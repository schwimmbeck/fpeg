// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The exceptions of the package, and the diagnostics that a grammar error carries and a
//! check returns.

use fpeg::{Limit, LineIndex, ParseErrorKind, Severity, Source};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedStr;
use pyo3::types::{PyDict, PyString, PyTuple};
use pyo3::{create_exception, intern};

/// The input does not match the grammar.
///
/// `message` says what is wrong, `pos`, `line` and `column` where it starts and `end_pos`,
/// `end_line` and `end_column` where it ends, `pos` counting code points from 0 and lines and
/// columns from 1. `expected` lists what the grammar expected there, and `label` is the name of
/// a named error. A parse with `recover` gives the recovered tree as `tree` and every error as
/// `errors`.
#[pyclass(extends = PyValueError, subclass, frozen, module = "fpeg")]
pub struct ParseError;

#[pymethods]
impl ParseError {
    #[new]
    #[pyo3(signature = (*_args, **_kwds))]
    fn new(_args: &Bound<'_, PyTuple>, _kwds: Option<&Bound<'_, PyDict>>) -> ParseError {
        ParseError
    }

    /// The errors as the command line shows them, each with the line of the text it stands in
    /// and carets under its span, and a blank line between them: those of `errors`, or this one
    /// where it lists none. `path` names the text. An error that no parse raised shows its first
    /// line alone.
    #[pyo3(signature = (path = None))]
    fn render(slf: &Bound<'_, Self>, path: Option<&str>) -> PyResult<String> {
        let py = slf.py();
        let Some(text) = slf.getattr_opt(intern!(py, "_text"))? else {
            return Ok(fpeg::error_title(None, &slf.str()?.to_string_lossy()));
        };
        let text: PyBackedStr = text.extract()?;
        let source = Source::new(&text, path);
        let errors = slf.getattr(intern!(py, "errors"))?;
        let mut shown: Vec<Bound<'_, PyAny>> = errors.try_iter()?.collect::<PyResult<_>>()?;
        if shown.is_empty() {
            shown.push(slf.clone().into_any());
        }
        let blocks = (shown.iter())
            .map(|error| {
                let label: Option<String> = error.getattr(intern!(py, "label"))?.extract()?;
                let message: String = error.getattr(intern!(py, "message"))?.extract()?;
                let start = source.byte_offset(error.getattr(intern!(py, "pos"))?.extract()?);
                let end = source.byte_offset(error.getattr(intern!(py, "end_pos"))?.extract()?);
                Ok(source.render(label.as_deref(), &message, start, end))
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(blocks.join("\n\n"))
    }
}

/// A parse stopped at a limit on its depth, its steps, its memory or the size of its input,
/// which `limit` names: `"depth"`, `"steps"`, `"memory"` or `"input size"`.
#[pyclass(extends = ParseError, subclass, frozen, module = "fpeg")]
pub struct LimitError;

#[pymethods]
impl LimitError {
    #[new]
    #[pyo3(signature = (*_args, **_kwds))]
    fn new(
        _args: &Bound<'_, PyTuple>,
        _kwds: Option<&Bound<'_, PyDict>>,
    ) -> PyClassInitializer<LimitError> {
        PyClassInitializer::from(ParseError).add_subclass(LimitError)
    }
}

create_exception!(
    fpeg,
    GrammarError,
    PyValueError,
    "A grammar has problems that keep it from compiling."
);

/// The path that messages show for a grammar given as text.
const TEXT_PATH: &str = "<grammar>";

/// An error or a warning about a grammar: its stable code, what exactly is wrong, and where.
/// Diagnostics with the same parts are equal.
#[pyclass(frozen, eq, hash, module = "fpeg", get_all)]
#[derive(PartialEq, Eq, Hash)]
pub struct Diagnostic {
    /// The stable code, such as `E201`, whose page in the documentation explains it.
    code: String,
    /// What exactly is wrong.
    message: String,
    /// Where the problem starts, line and column counted from 1 in code points.
    line: usize,
    column: usize,
    /// Where the problem ends.
    end_line: usize,
    end_column: usize,
    /// The grammar file, or `None` for a grammar given as text.
    path: Option<String>,
}

impl Diagnostic {
    /// The severity that the code tells, as the core has it.
    fn severity_of(&self) -> Severity {
        match self.code.starts_with('W') {
            true => Severity::Warning,
            false => Severity::Error,
        }
    }

    fn rendered(&self) -> String {
        let path = self.path.as_deref().unwrap_or(TEXT_PATH);
        format!(
            "{path}:{}:{}: {}[{}]: {}",
            self.line,
            self.column,
            self.severity_of(),
            self.code,
            self.message
        )
    }
}

#[pymethods]
impl Diagnostic {
    #[new]
    #[pyo3(signature = (code, message, line, column, end_line, end_column, path = None))]
    fn new(
        code: String,
        message: String,
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
        path: Option<String>,
    ) -> Diagnostic {
        Diagnostic {
            code,
            message,
            line,
            column,
            end_line,
            end_column,
            path,
        }
    }

    /// `"error"` for a problem that keeps the grammar from compiling, `"warning"` for a form
    /// that works and is likely not meant.
    #[getter]
    fn severity(&self) -> String {
        self.severity_of().to_string()
    }

    fn __str__(&self) -> String {
        self.rendered()
    }

    fn __repr__(&self) -> String {
        format!("<Diagnostic {}>", self.rendered())
    }

    /// Pickle a diagnostic as its fields, which pickles a grammar error with its diagnostics.
    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        let d = slf.get();
        let fields = (
            &d.code,
            &d.message,
            d.line,
            d.column,
            d.end_line,
            d.end_column,
            d.path.as_deref(),
        );
        (slf.get_type(), fields).into_pyobject(slf.py())
    }
}

/// The diagnostics of the core, placed in `text`.
pub fn diagnostics(text: &str, path: Option<&str>, found: &[fpeg::Diagnostic]) -> Vec<Diagnostic> {
    let lines = LineIndex::new(text);
    (found.iter())
        .map(|found| {
            let start = lines.position(text, found.span.start as usize);
            let end = lines.position(text, found.span.end as usize);
            Diagnostic {
                code: found.code.to_string(),
                message: found.message.clone(),
                line: start.line,
                column: start.column,
                end_line: end.line,
                end_column: end.column,
                path: path.map(str::to_owned),
            }
        })
        .collect()
}

/// The exception for the problems of a grammar, placed in `text`.
pub fn grammar_error(
    py: Python<'_>,
    text: &str,
    path: Option<&str>,
    errors: &[fpeg::Diagnostic],
) -> PyErr {
    let diagnostics = diagnostics(text, path, errors);
    let message = (diagnostics.iter())
        .map(Diagnostic::rendered)
        .collect::<Vec<_>>()
        .join("\n");
    let error = GrammarError::new_err(message);
    let set = || -> PyResult<()> {
        let diagnostics = (diagnostics.into_iter())
            .map(|diagnostic| Py::new(py, diagnostic))
            .collect::<PyResult<Vec<_>>>()?;
        let value = error.value(py);
        value.setattr(intern!(py, "diagnostics"), PyTuple::new(py, diagnostics)?)
    };
    match set() {
        Ok(()) => error,
        Err(failure) => failure,
    }
}

/// One error a parse found, as its exception reports it.
struct Found {
    start: usize,
    end: usize,
    message: String,
    label: Option<String>,
    expected: Vec<String>,
    /// The limit that a limit error names.
    limit: Option<&'static str>,
}

impl Found {
    /// The error that ended a parse.
    fn of_error(error: &fpeg::ParseError) -> Found {
        let (expected, label) = match &error.kind {
            ParseErrorKind::NoMatch { expected } => (expected.clone(), None),
            ParseErrorKind::Label {
                label,
                named,
                expected,
            } => (expected.clone(), named.then(|| label.clone())),
            _ => (Vec::new(), None),
        };
        let limit = match &error.kind {
            ParseErrorKind::Limit(Limit::Depth) => Some("depth"),
            ParseErrorKind::Limit(Limit::Steps) => Some("steps"),
            ParseErrorKind::Limit(Limit::InputSize) => Some("input size"),
            ParseErrorKind::Limit(Limit::Memory) => Some("memory"),
            ParseErrorKind::Limit(_) => Some("limit"),
            _ => None,
        };
        Found {
            start: error.position,
            end: error.end(),
            message: error.message(),
            label,
            expected,
            limit,
        }
    }

    /// A labeled error that a parse recovered from.
    fn of_recovery(recovery: &fpeg::Recovery) -> Found {
        Found {
            start: recovery.position,
            end: recovery.end.max(recovery.position),
            message: recovery.message(),
            label: Some(recovery.label.clone()),
            expected: recovery.expected.clone(),
            limit: None,
        }
    }
}

/// The errors of a parse, each placed in the source, and the exceptions that report them.
struct Report<'s, 'py> {
    source: &'s str,
    /// The source as the caller gave it, which each exception keeps to show its error in.
    text: &'s Bound<'py, PyString>,
    lines: LineIndex,
}

impl<'py> Report<'_, 'py> {
    /// Where an error starts, and its message, as one line of text.
    fn line(&self, found: &Found) -> String {
        let start = self.lines.position(self.source, found.start);
        format!(
            "line {}, column {}: {}",
            start.line, start.column, found.message
        )
    }

    /// The exception for one error, with `text` as what it says, and no tree and no errors of
    /// its own.
    fn exception(
        &self,
        py: Python<'py>,
        found: &Found,
        text: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let value = match found.limit {
            Some(_) => py.get_type::<LimitError>().call1((text,))?,
            None => py.get_type::<ParseError>().call1((text,))?,
        };
        let start = self.lines.position(self.source, found.start);
        let end = self.lines.position(self.source, found.end);
        value.setattr(intern!(py, "message"), &found.message)?;
        value.setattr(intern!(py, "pos"), start.offset)?;
        value.setattr(intern!(py, "line"), start.line)?;
        value.setattr(intern!(py, "column"), start.column)?;
        value.setattr(intern!(py, "end_pos"), end.offset)?;
        value.setattr(intern!(py, "end_line"), end.line)?;
        value.setattr(intern!(py, "end_column"), end.column)?;
        value.setattr(intern!(py, "expected"), PyTuple::new(py, &found.expected)?)?;
        value.setattr(intern!(py, "label"), &found.label)?;
        if let Some(limit) = found.limit {
            value.setattr(intern!(py, "limit"), limit)?;
        }
        value.setattr(intern!(py, "tree"), py.None())?;
        value.setattr(intern!(py, "errors"), PyTuple::empty(py))?;
        value.setattr(intern!(py, "_text"), self.text)?;
        Ok(value)
    }

    /// The exception raised for the errors found: the one at `main` describes it, it says every
    /// error on a line of its own, lists each as an exception of its own under `errors`, and
    /// holds the tree the parse recovered.
    fn raised(
        &self,
        py: Python<'py>,
        found: &[Found],
        main: usize,
        tree: Option<Py<PyAny>>,
    ) -> PyResult<PyErr> {
        let lines: Vec<String> = found.iter().map(|f| self.line(f)).collect();
        let errors = (found.iter().zip(&lines))
            .map(|(f, line)| self.exception(py, f, line.clone()))
            .collect::<PyResult<Vec<_>>>()?;
        let raised = self.exception(py, &found[main], lines.join("\n"))?;
        raised.setattr(intern!(py, "errors"), PyTuple::new(py, errors)?)?;
        raised.setattr(intern!(py, "tree"), tree)?;
        Ok(PyErr::from_value(raised))
    }
}

/// The exception for a parse of `text`, also given as `source`, that produced no tree: its
/// error, after the errors a recovering parse recovered from on its way.
pub fn parse_error(
    py: Python<'_>,
    text: &Bound<'_, PyString>,
    source: &str,
    error: &fpeg::ParseError,
) -> PyErr {
    let report = Report {
        source,
        text,
        lines: LineIndex::new(source),
    };
    let mut found: Vec<Found> = error.recoveries.iter().map(Found::of_recovery).collect();
    found.push(Found::of_error(error));
    report
        .raised(py, &found, found.len() - 1, None)
        .unwrap_or_else(|failure| failure)
}

/// The exception for a parse of `text`, also given as `source`, that recovered from errors:
/// the first error describes it, and `tree` holds the tree with the error items.
pub fn recovered_error(
    py: Python<'_>,
    text: &Bound<'_, PyString>,
    source: &str,
    recoveries: &[fpeg::Recovery],
    tree: Py<PyAny>,
) -> PyErr {
    let report = Report {
        source,
        text,
        lines: LineIndex::new(source),
    };
    let found: Vec<Found> = recoveries.iter().map(Found::of_recovery).collect();
    report
        .raised(py, &found, 0, Some(tree))
        .unwrap_or_else(|failure| failure)
}
