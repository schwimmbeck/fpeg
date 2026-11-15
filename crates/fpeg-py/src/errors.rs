// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The exceptions of the package, and the diagnostics that a grammar error carries and a
//! check returns.

use fpeg::{Limit, LineIndex, ParseErrorKind, Severity};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use pyo3::{create_exception, intern};

create_exception!(
    fpeg,
    ParseError,
    PyValueError,
    "The input does not match the grammar."
);
create_exception!(
    fpeg,
    LimitError,
    ParseError,
    "A parse stopped at a limit on its depth, its steps, its memory or the size of its input."
);
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
            ParseErrorKind::Label(label) => (Vec::new(), Some(label.clone())),
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
            end: error.position,
            message: error.message(),
            label,
            expected,
            limit,
        }
    }

    /// A labeled error that a parse recovered from.
    fn of_recovery(recovery: &fpeg::Recovery) -> Found {
        Found {
            start: recovery.start,
            end: recovery.end,
            message: recovery.label.clone(),
            label: Some(recovery.label.clone()),
            expected: Vec::new(),
            limit: None,
        }
    }
}

/// The errors of a parse, each placed in the source, and the exceptions that report them.
struct Report<'s> {
    source: &'s str,
    lines: LineIndex,
}

impl Report<'_> {
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
    fn exception(&self, py: Python<'_>, found: &Found, text: String) -> PyResult<PyErr> {
        let exception = match found.limit {
            Some(_) => LimitError::new_err(text),
            None => ParseError::new_err(text),
        };
        let value = exception.value(py);
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
        Ok(exception)
    }

    /// The exception raised for the errors found: the one at `main` describes it, it says every
    /// error on a line of its own, lists each as an exception of its own under `errors`, and
    /// holds the tree the parse recovered.
    fn raised(
        &self,
        py: Python<'_>,
        found: &[Found],
        main: usize,
        tree: Option<Py<PyAny>>,
    ) -> PyResult<PyErr> {
        let lines: Vec<String> = found.iter().map(|f| self.line(f)).collect();
        let errors = (found.iter().zip(&lines))
            .map(|(f, line)| Ok(self.exception(py, f, line.clone())?.into_value(py)))
            .collect::<PyResult<Vec<_>>>()?;
        let raised = self.exception(py, &found[main], lines.join("\n"))?;
        let value = raised.value(py);
        value.setattr(intern!(py, "errors"), PyTuple::new(py, errors)?)?;
        value.setattr(intern!(py, "tree"), tree)?;
        Ok(raised)
    }
}

/// The exception for a parse that produced no tree, placed in `source`: its error, after the
/// errors a recovering parse recovered from on its way.
pub fn parse_error(py: Python<'_>, source: &str, error: &fpeg::ParseError) -> PyErr {
    let report = Report {
        source,
        lines: LineIndex::new(source),
    };
    let mut found: Vec<Found> = error.recoveries.iter().map(Found::of_recovery).collect();
    found.push(Found::of_error(error));
    report
        .raised(py, &found, found.len() - 1, None)
        .unwrap_or_else(|failure| failure)
}

/// The exception for a parse that recovered from errors, placed in `source`: the first error
/// describes it, and `tree` holds the tree with the error items.
pub fn recovered_error(
    py: Python<'_>,
    source: &str,
    recoveries: &[fpeg::Recovery],
    tree: Py<PyAny>,
) -> PyErr {
    let report = Report {
        source,
        lines: LineIndex::new(source),
    };
    let found: Vec<Found> = recoveries.iter().map(Found::of_recovery).collect();
    report
        .raised(py, &found, 0, Some(tree))
        .unwrap_or_else(|failure| failure)
}
