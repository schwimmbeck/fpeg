// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Grammar`: a grammar compiled for the parsing machine, and parses with it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};

use fpeg::{
    DEFAULT_MAX_DEPTH, DEFAULT_MAX_STEPS, DiagnosticCode, FieldCount, FieldId, Limits, Parser,
};
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyUnicodeDecodeError, PyValueError};
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedStr;
use pyo3::types::{PyString, PyType};

use crate::builders::{self, Builder};
use crate::errors::{grammar_error, parse_error};
use crate::transform::Plans;
use crate::tree::{Parsed, item};

/// Inputs from this size on are parsed with the thread detached from the interpreter, so that
/// other Python threads run meanwhile. Detaching costs 37 ns uncontended on 3.15 and 13 ns on
/// the free-threaded build (measured), under 0.1 percent of a parse this long, which takes about
/// 100 us. A shorter parse keeps the interpreter, since taking it back from a busy thread can
/// wait far longer than the parse. hashlib likewise keeps it for data below 2 KiB.
const DETACH_MIN_BYTES: usize = 4096;

/// The serial of the next grammar. Tokens carry the serial of the grammar that made them.
static NEXT_SERIAL: AtomicU64 = AtomicU64::new(1);

/// The live grammars by serial, for the builder of a token on its own.
static GRAMMARS: LazyLock<Mutex<HashMap<u64, Weak<GrammarCore>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The live grammar with this serial.
pub fn grammar_of(serial: u64) -> Option<Arc<GrammarCore>> {
    let grammars = GRAMMARS.lock().unwrap_or_else(PoisonError::into_inner);
    grammars.get(&serial).and_then(Weak::upgrade)
}

/// What a compiled grammar holds for its parses and their views.
pub struct GrammarCore {
    pub parser: Parser,
    pub serial: u64,
    /// The name of each kind, interned, by kind id.
    pub kind_names: Vec<Py<PyString>>,
    /// The name of each field, by field id.
    pub field_names: Vec<Py<PyString>>,
    pub field_ids: HashMap<String, FieldId>,
    /// For each kind, the fields of all kinds of its name with the largest count of each, which
    /// is what a node of that name shows by field.
    pub name_fields: Vec<Vec<(FieldId, FieldCount)>>,
    /// The plans of the transformer classes that met the grammar.
    pub plans: Plans,
    /// The value builder of each kind, by kind id.
    pub builders: Vec<Option<Builder>>,
}

impl Drop for GrammarCore {
    fn drop(&mut self) {
        let mut grammars = GRAMMARS.lock().unwrap_or_else(PoisonError::into_inner);
        grammars.remove(&self.serial);
    }
}

impl GrammarCore {
    fn new(
        py: Python<'_>,
        parser: Parser,
        functions: &HashMap<String, Py<PyAny>>,
    ) -> PyResult<GrammarCore> {
        let kinds = parser.kinds();
        let kind_names = (kinds.iter())
            .map(|kind| PyString::intern(py, &kind.name).unbind())
            .collect();
        let names = parser.names();
        let field_names = (names.fields.iter())
            .map(|name| PyString::intern(py, name).unbind())
            .collect();
        let field_ids: HashMap<String, FieldId> = (names.fields.iter().enumerate())
            .skip(1)
            .filter_map(|(id, name)| Some((name.clone(), FieldId(u16::try_from(id).ok()?))))
            .collect();
        let mut by_name: HashMap<&str, HashMap<FieldId, FieldCount>> = HashMap::new();
        for kind in kinds {
            let fields = by_name.entry(&kind.name).or_default();
            for &(field, count) in &kind.fields {
                let held = fields.entry(field).or_insert(count);
                *held = (*held).max(count);
            }
        }
        // A method meets every node of its name, so a field that some of them lack can be absent.
        for kind in kinds.iter().filter(|kind| kind.made && !kind.token) {
            let Some(fields) = by_name.get_mut(kind.name.as_str()) else {
                continue;
            };
            for (field, count) in fields.iter_mut() {
                if kind.fields.iter().all(|&(f, _)| f != *field) {
                    *count = (*count).max(FieldCount::Optional);
                }
            }
        }
        let name_fields = (kinds.iter())
            .map(|kind| {
                let mut fields: Vec<(FieldId, FieldCount)> = (by_name.get(kind.name.as_str()))
                    .map(|fields| fields.iter().map(|(&f, &c)| (f, c)).collect())
                    .unwrap_or_default();
                fields.sort();
                fields
            })
            .collect();
        let mut resolved = Vec::new();
        for kind in kinds {
            resolved.push(match &kind.builder {
                Some(builder) => Some(builders::resolve(
                    py,
                    builder,
                    &kind.fields,
                    &field_ids,
                    functions,
                )?),
                None => None,
            });
        }
        Ok(GrammarCore {
            parser,
            serial: NEXT_SERIAL.fetch_add(1, Ordering::Relaxed),
            kind_names,
            field_names,
            field_ids,
            name_fields,
            plans: Plans::default(),
            builders: resolved,
        })
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
    fn compile(
        py: Python<'_>,
        text: &str,
        path: Option<&str>,
        functions: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Grammar> {
        let grammar = fpeg::Grammar::from_text(text)
            .map_err(|errors| grammar_error(py, text, path, &errors))?;
        let parser =
            Parser::new(&grammar).map_err(|errors| grammar_error(py, text, path, &errors))?;
        let functions = given_functions(functions)?;
        let mut missing: Vec<fpeg::GrammarError> = Vec::new();
        for kind in parser.kinds() {
            let name = kind.builder.as_ref().and_then(builders::function_name);
            if let Some(name) = name.filter(|name| !functions.contains_key(&name.text)) {
                let error = fpeg::GrammarError {
                    code: DiagnosticCode::BuilderFunction,
                    message: format!(
                        "the builder calls {}, which is not among the functions given",
                        name.text
                    ),
                    span: name.span,
                };
                if !missing.contains(&error) {
                    missing.push(error);
                }
            }
        }
        if !missing.is_empty() {
            missing.sort_by_key(|error| error.span.start);
            return Err(grammar_error(py, text, path, &missing));
        }
        let core = Arc::new(GrammarCore::new(py, parser, &functions)?);
        let mut grammars = GRAMMARS.lock().unwrap_or_else(PoisonError::into_inner);
        grammars.insert(core.serial, Arc::downgrade(&core));
        drop(grammars);
        Ok(Grammar { core })
    }
}

/// The functions given with a grammar, by name: a mapping of str to callables.
fn given_functions(functions: Option<&Bound<'_, PyAny>>) -> PyResult<HashMap<String, Py<PyAny>>> {
    let mut given = HashMap::new();
    let Some(functions) = functions.filter(|f| !f.is_none()) else {
        return Ok(given);
    };
    for item in functions.call_method0("items")?.try_iter()? {
        let (name, function): (String, Bound<'_, PyAny>) = item?.extract()?;
        if !function.is_callable() {
            let message = format!("the function {name} given with the grammar is not callable");
            return Err(PyTypeError::new_err(message));
        }
        given.insert(name, function.unbind());
    }
    Ok(given)
}

#[pymethods]
impl Grammar {
    /// Compile a grammar given as text, or raise `GrammarError` with its problems.
    ///
    /// `functions` maps the names of the functions that the grammar's value builders call
    /// to the callables, every one of them given.
    #[new]
    #[pyo3(signature = (text, *, functions = None))]
    fn new(py: Python<'_>, text: &str, functions: Option<&Bound<'_, PyAny>>) -> PyResult<Grammar> {
        Grammar::compile(py, text, None, functions)
    }

    /// Read a grammar file, UTF-8 with or without a byte order mark, and compile it.
    #[classmethod]
    #[pyo3(signature = (path, *, functions = None))]
    fn open(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        path: PathBuf,
        functions: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Grammar> {
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
        Grammar::compile(py, text, Some(&path.display().to_string()), functions)
    }

    /// Parse the whole text from the rule `start` and return its one visible item at the top:
    /// a `Tree` for a node, a `Token` for a token. Raises `ParseError` when the text does not
    /// match, and `LimitError` when the parse takes more steps or nests deeper than allowed.
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
        let tree = result.map_err(|error| parse_error(py, &source, &error))?;
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

    /// The Python source of a starting transformer for the grammar: a method for every rule that
    /// makes nodes and every terminal, the parameters of a node's method named after its fields,
    /// with type hints from how many items each field holds, and `strict=True`, so that a
    /// misspelled method fails.
    #[pyo3(signature = (name = "Transformer"))]
    fn transformer_template(&self, py: Python<'_>, name: &str) -> PyResult<String> {
        let keywords: Vec<String> = py.import("keyword")?.getattr("kwlist")?.extract()?;
        let is_identifier = |text: &str| -> PyResult<bool> {
            PyString::new(py, text)
                .call_method0("isidentifier")?
                .extract()
        };
        let core = &self.core;
        let kinds = core.parser.kinds();
        let mut out = format!(
            "from __future__ import annotations\n\nfrom typing import Any\n\nimport fpeg\n\n\nclass {name}(fpeg.Transformer, strict=True):\n    \"\"\"Values for the nodes and tokens of the grammar.\"\"\"\n"
        );
        let mut seen = std::collections::HashSet::new();
        for (id, kind) in kinds.iter().enumerate() {
            let kind_name = kind.name.as_str();
            // Rules `_name` make no node, and hidden terminals no visible token. Nor does a
            // transparent rule that always gives way to its one child.
            let made = kinds.iter().any(|k| k.name == kind_name && k.made);
            if kind_name.starts_with('_')
                || !made
                || !is_identifier(kind_name)?
                || !seen.insert(kind_name)
            {
                continue;
            }
            out.push('\n');
            if keywords.iter().any(|k| k == kind_name) || kind_name == "transform" {
                out.push_str(&format!(
                    "    # {kind_name} cannot be a method of the class: Python reserves the name.\n"
                ));
                continue;
            }
            let mut builders: Vec<String> = (kinds.iter())
                .filter(|k| k.name == kind_name && k.made)
                .filter_map(|k| k.builder.as_ref().map(|builder| format!("=> {builder}")))
                .collect();
            builders.dedup();
            // A summary line, then a line for each field and for a builder the method replaces.
            let mut lines: Vec<String> = Vec::new();
            let fields: Vec<(String, FieldCount)> = (core.name_fields[id].iter())
                .map(|&(field, count)| {
                    let field_name = core.field_names[usize::from(field.0)].bind(py).to_string();
                    (field_name, count)
                })
                .collect();
            // A field named like a keyword or `self` cannot be a parameter, and one named
            // `__x` would be mangled in the class.
            let fields_fit = (fields.iter()).all(|(field, _)| {
                !keywords.contains(field) && field != "self" && !field.starts_with("__")
            });
            let signature = if kind.token {
                lines.push(format!("Return the value of a token {kind_name}."));
                "self, token: fpeg.Token".to_owned()
            } else if fields.is_empty() || !fields_fit {
                lines.push(format!(
                    "Return the value of a node {kind_name} from its children."
                ));
                "self, *children: Any".to_owned()
            } else {
                lines.push(format!("Return the value of a node {kind_name}."));
                lines.push(String::new());
                let mut parameters = vec!["self".to_owned()];
                for (field, count) in &fields {
                    let (hint, held) = match count {
                        FieldCount::One => ("Any", "one item"),
                        FieldCount::Optional => ("Any | None", "one item or None"),
                        FieldCount::Many => ("list[Any]", "a list of items"),
                    };
                    parameters.push(format!("{field}: {hint}"));
                    lines.push(format!("{field} holds {held}."));
                }
                parameters.join(", ")
            };
            if !builders.is_empty() {
                if lines.len() == 1 {
                    lines.push(String::new());
                }
                let (what, verb) = if builders.len() == 1 {
                    ("builder", "applies")
                } else {
                    ("builders", "apply")
                };
                lines.push(format!(
                    "Without this method the grammar's {what} {} {verb}.",
                    builders.join(", ")
                ));
            }
            // Builder text shows escapes, which a raw docstring keeps as written.
            let raw = if lines.iter().any(|line| line.contains('\\')) {
                "r"
            } else {
                ""
            };
            let doc = if lines.len() == 1 {
                format!("        {raw}\"\"\"{}\"\"\"\n", lines[0])
            } else {
                let body: Vec<String> = (lines.iter().skip(1))
                    .map(|line| {
                        if line.is_empty() {
                            String::new()
                        } else {
                            format!("        {line}")
                        }
                    })
                    .collect();
                format!(
                    "        {raw}\"\"\"{}\n{}\n        \"\"\"\n",
                    lines[0],
                    body.join("\n")
                )
            };
            out.push_str(&format!(
                "    def {kind_name}({signature}) -> Any:\n{doc}        ...\n"
            ));
        }
        Ok(out)
    }

    fn __repr__(&self) -> String {
        format!("<fpeg.Grammar with {} kinds>", self.core.kind_names.len())
    }
}
