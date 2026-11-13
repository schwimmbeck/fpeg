// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Grammar`: a grammar compiled for the parsing machine, and parses with it.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use fpeg::{DEFAULT_MAX_DEPTH, DEFAULT_MAX_STEPS, Limits, Parser};
use pyo3::exceptions::{PyRuntimeError, PyUnicodeDecodeError, PyValueError};
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedStr;
use pyo3::types::{PyString, PyType};

use crate::errors::grammar_error;
use crate::tree::{Parsed, item};

/// Inputs from this size on are parsed with the thread detached from the interpreter, so that
/// other Python threads run meanwhile. Below it the parse is too short for detaching to pay.
const DETACH_MIN_BYTES: usize = 4096;

/// The serial of the next grammar. Tokens carry the serial of the grammar that made them.
static NEXT_SERIAL: AtomicU64 = AtomicU64::new(1);

/// What a compiled grammar holds for its parses and their views.
pub struct GrammarCore {
    pub parser: Parser,
    pub serial: u64,
    /// The name of each kind, interned, by kind id.
    pub kind_names: Vec<Py<PyString>>,
}

impl GrammarCore {
    fn new(py: Python<'_>, parser: Parser) -> GrammarCore {
        let kinds = parser.kinds();
        let kind_names = (kinds.iter())
            .map(|kind| PyString::intern(py, &kind.name).unbind())
            .collect();
        GrammarCore {
            parser,
            serial: NEXT_SERIAL.fetch_add(1, Ordering::Relaxed),
            kind_names,
        }
    }
}

/// A grammar compiled for fpeg's parsing machine.
///
/// It is immutable, so one grammar serves any number of parses, from any thread.
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
            core: Arc::new(GrammarCore::new(py, parser)),
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

    /// Parse the whole text from the rule `start` and return its one visible item at the top:
    /// a `Tree` for a node, a `Token` for a token. Raises `ValueError` when the text does not
    /// match, or when the parse takes more steps or nests deeper than allowed.
    #[pyo3(signature = (text, start = "start", *, max_steps = None, max_depth = None))]
    fn parse(
        &self,
        py: Python<'_>,
        text: Bound<'_, PyString>,
        start: &str,
        max_steps: Option<u64>,
        max_depth: Option<u32>,
    ) -> PyResult<Py<PyAny>> {
        let core = &self.core;
        match core.parser.single_item(start) {
            None => {
                let message = format!("the grammar has no rule {start} to start from");
                return Err(PyValueError::new_err(message));
            }
            Some(false) => {
                let message = format!(
                    "a parse from {start} can leave no visible item or several at the top, so \
                     it cannot return one"
                );
                return Err(PyValueError::new_err(message));
            }
            Some(true) => {}
        }
        let source = PyBackedStr::try_from(text)?;
        let limits = Limits {
            max_steps: max_steps.unwrap_or(DEFAULT_MAX_STEPS),
            max_depth: max_depth.unwrap_or(DEFAULT_MAX_DEPTH),
        };
        let result = if source.len() >= DETACH_MIN_BYTES {
            py.detach(|| core.parser.parse_with(&source, start, limits))
        } else {
            core.parser.parse_with(&source, start, limits)
        };
        let tree = result.map_err(|error| PyValueError::new_err(error.message()))?;
        let parsed = Arc::new(Parsed::new(tree, source, Arc::clone(core)));
        let root = (parsed.tree.roots().into_iter())
            .find(|root| !root.is_hidden())
            .map(|root| root.index());
        match root {
            Some(index) => item(py, &parsed, index),
            None => Err(PyRuntimeError::new_err(
                "a bug in fpeg: the parse left no visible item at the top",
            )),
        }
    }

    fn __repr__(&self) -> String {
        format!("<fpeg.Grammar with {} kinds>", self.core.kind_names.len())
    }
}
