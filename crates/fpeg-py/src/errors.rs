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
    "A parse stopped at a limit on its depth, its steps or the size of its input."
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
#[pyclass(frozen, module = "fpeg", get_all)]
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

/// The exception for a parse that produced no tree, placed in `source`.
pub fn parse_error(py: Python<'_>, source: &str, error: &fpeg::ParseError) -> PyErr {
    let position = LineIndex::new(source).position(source, error.position);
    let message = error.message();
    let text = format!(
        "line {}, column {}: {message}",
        position.line, position.column
    );
    let (exception, limit) = match &error.kind {
        ParseErrorKind::Limit(limit) => {
            let name = match limit {
                Limit::Depth => "depth",
                Limit::Steps => "steps",
                Limit::InputSize => "input size",
            };
            (LimitError::new_err(text), Some(name))
        }
        _ => (ParseError::new_err(text), None),
    };
    let set = || -> PyResult<()> {
        let value = exception.value(py);
        let (expected, label) = match &error.kind {
            ParseErrorKind::NoMatch { expected } => (expected.clone(), None),
            ParseErrorKind::Label(label) => (Vec::new(), Some(label.clone())),
            _ => (Vec::new(), None),
        };
        value.setattr(intern!(py, "message"), message)?;
        value.setattr(intern!(py, "pos"), position.offset)?;
        value.setattr(intern!(py, "line"), position.line)?;
        value.setattr(intern!(py, "column"), position.column)?;
        value.setattr(intern!(py, "expected"), PyTuple::new(py, expected)?)?;
        value.setattr(intern!(py, "label"), label)?;
        if let Some(limit) = limit {
            value.setattr(intern!(py, "limit"), limit)?;
        }
        Ok(())
    };
    match set() {
        Ok(()) => exception,
        Err(failure) => failure,
    }
}
