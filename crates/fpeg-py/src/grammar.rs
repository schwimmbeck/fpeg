// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Grammar`: a grammar compiled for the parsing machine.

use std::path::PathBuf;
use std::sync::Arc;

use fpeg::Parser;
use pyo3::exceptions::PyUnicodeDecodeError;
use pyo3::prelude::*;
use pyo3::types::PyType;

use crate::errors::grammar_error;

/// What a compiled grammar holds.
pub struct GrammarCore {
    pub parser: Parser,
}

/// A grammar compiled for fpeg's parsing machine.
///
/// It is immutable, so one grammar can serve any number of threads.
#[pyclass(frozen, module = "fpeg")]
pub struct Grammar {
    pub core: Arc<GrammarCore>,
}

impl Grammar {
    fn compile(py: Python<'_>, text: &str, path: Option<&str>) -> PyResult<Grammar> {
        let grammar = fpeg::Grammar::from_text(text)
            .map_err(|errors| grammar_error(py, text, path, &errors))?;
        let parser =
            Parser::new(&grammar).map_err(|errors| grammar_error(py, text, path, &errors))?;
        Ok(Grammar {
            core: Arc::new(GrammarCore { parser }),
        })
    }
}

#[pymethods]
impl Grammar {
    /// Compile a grammar given as text, or raise `GrammarError` with its problems.
    #[new]
    fn new(py: Python<'_>, text: &str) -> PyResult<Grammar> {
        Grammar::compile(py, text, None)
    }

    /// Read a grammar file, UTF-8 with or without a byte order mark, and compile it.
    #[classmethod]
    fn open(_cls: &Bound<'_, PyType>, py: Python<'_>, path: PathBuf) -> PyResult<Grammar> {
        let bytes = std::fs::read(&path)?;
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(error) => {
                return Err(PyErr::from_value(
                    PyUnicodeDecodeError::new_utf8(py, &bytes, error)?.into_any(),
                ));
            }
        };
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        Grammar::compile(py, text, Some(&path.display().to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "<fpeg.Grammar with {} kinds>",
            self.core.parser.kinds().len()
        )
    }
}
