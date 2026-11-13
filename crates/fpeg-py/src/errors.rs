// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The exception for a grammar with problems, and the diagnostics it carries.

use fpeg::LineIndex;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use pyo3::{create_exception, intern};

create_exception!(
    fpeg,
    GrammarError,
    PyValueError,
    "A grammar has problems that keep it from compiling."
);

/// The path that messages show for a grammar given as text.
const TEXT_PATH: &str = "<grammar>";

/// A problem of a grammar: its stable code, what exactly is wrong, and where.
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
    fn rendered(&self) -> String {
        let path = self.path.as_deref().unwrap_or(TEXT_PATH);
        format!(
            "{path}:{}:{}: error[{}]: {}",
            self.line, self.column, self.code, self.message
        )
    }
}

#[pymethods]
impl Diagnostic {
    fn __str__(&self) -> String {
        self.rendered()
    }

    fn __repr__(&self) -> String {
        format!("<Diagnostic {}>", self.rendered())
    }
}

/// The exception for the problems of a grammar, placed in `text`.
pub fn grammar_error(
    py: Python<'_>,
    text: &str,
    path: Option<&str>,
    errors: &[fpeg::GrammarError],
) -> PyErr {
    let lines = LineIndex::new(text);
    let mut diagnostics = Vec::new();
    for error in errors {
        let start = lines.position(text, error.span.start as usize);
        let end = lines.position(text, error.span.end as usize);
        diagnostics.push(Diagnostic {
            code: error.code.to_string(),
            message: error.message.clone(),
            line: start.line,
            column: start.column,
            end_line: end.line,
            end_column: end.column,
            path: path.map(str::to_owned),
        });
    }
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
