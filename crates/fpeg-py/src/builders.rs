// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The value builders of a grammar, `=>` after an alternative, run natively when no method of a
//! transformer handles the node or token.
//!
//! `int` and `float` take fast paths for plain ASCII numbers and Python's own conversion for
//! anything else, so their values are Python's: Rust parses decimal floats correctly rounded, as
//! CPython does. `unquote` decodes JSON's escapes as `json.loads` does, lone surrogates included.

use std::collections::HashMap;

use fpeg::{BuilderValue, Constant, FieldCount, FieldId, ValueBuilder};
use pyo3::PyTypeInfo;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};

/// Digits an `i64` holds whatever they are, so that a number this long parses without overflow.
const I64_SAFE_DIGITS: usize = 18;

/// A builder resolved for the binding: built-ins by what they do, functions as the objects
/// given with the grammar, fields by id.
pub enum Builder {
    Int,
    Float,
    Text,
    Unquote,
    List,
    Tuple,
    Dict,
    Constant(Py<PyAny>),
    /// A function used bare: called with a token's text or a node's list of children.
    Bare {
        name: String,
        function: Py<PyAny>,
    },
    /// A function called with arguments.
    Call {
        name: String,
        function: Py<PyAny>,
        args: Vec<Arg>,
    },
}

pub struct Arg {
    keyword: Option<Py<PyString>>,
    value: ArgValue,
}

enum ArgValue {
    Field(FieldId, FieldCount),
    Constant(Py<PyAny>),
    Children,
}

impl Builder {
    /// The builder's name, for messages.
    pub fn name(&self) -> &str {
        match self {
            Builder::Int => "int",
            Builder::Float => "float",
            Builder::Text => "text",
            Builder::Unquote => "unquote",
            Builder::List => "list",
            Builder::Tuple => "tuple",
            Builder::Dict => "dict",
            Builder::Constant(_) => "constant",
            Builder::Bare { name, .. } | Builder::Call { name, .. } => name,
        }
    }
}

/// The name of the function a builder calls, if it calls one.
pub fn function_name(builder: &ValueBuilder) -> Option<&fpeg::Name> {
    match builder {
        ValueBuilder::Name(name) if !fpeg::BUILT_IN_BUILDERS.contains(&name.text.as_str()) => {
            Some(name)
        }
        ValueBuilder::Call { name, .. } => Some(name),
        ValueBuilder::Name(_) | ValueBuilder::Constant(_) => None,
    }
}

/// The value of a constant written in a builder. A number with a fraction is a float, any other
/// an int of any size.
fn constant(py: Python<'_>, value: &Constant) -> PyResult<Py<PyAny>> {
    Ok(match value {
        Constant::None => py.None(),
        Constant::Bool(value) => value.into_pyobject(py)?.to_owned().into_any().unbind(),
        Constant::Number(text) if text.contains('.') => {
            PyFloat::type_object(py).call1((text,))?.unbind()
        }
        Constant::Number(text) => PyInt::type_object(py).call1((text,))?.unbind(),
        Constant::String(text) => crate::string(py, text)?.into_any().unbind(),
    })
}

/// Resolve the builder of a kind whose fields are `fields`. The caller has checked that every
/// function it calls is among `functions`.
pub fn resolve(
    py: Python<'_>,
    builder: &ValueBuilder,
    fields: &[(FieldId, FieldCount)],
    field_ids: &HashMap<String, FieldId>,
    functions: &HashMap<String, Py<PyAny>>,
) -> PyResult<Builder> {
    let function = |name: &fpeg::Name| {
        (functions.get(&name.text))
            .map(|function| function.clone_ref(py))
            .ok_or_else(|| PyValueError::new_err(format!("no function {} was given", name.text)))
    };
    Ok(match builder {
        ValueBuilder::Constant(value) => Builder::Constant(constant(py, value)?),
        ValueBuilder::Name(name) => match name.text.as_str() {
            "int" => Builder::Int,
            "float" => Builder::Float,
            "text" => Builder::Text,
            "unquote" => Builder::Unquote,
            "list" => Builder::List,
            "tuple" => Builder::Tuple,
            "dict" => Builder::Dict,
            _ => Builder::Bare {
                name: name.text.clone(),
                function: function(name)?,
            },
        },
        ValueBuilder::Call { name, args } => {
            let mut resolved = Vec::new();
            for arg in args {
                let keyword =
                    (arg.key.as_ref()).map(|key| PyString::intern(py, &key.text).unbind());
                let value = match &arg.value {
                    BuilderValue::All => ArgValue::Children,
                    BuilderValue::Constant(value) => ArgValue::Constant(constant(py, value)?),
                    BuilderValue::Field(field) => {
                        let id = field_ids.get(&field.text).copied();
                        let known = id.and_then(|id| fields.iter().find(|&&(f, _)| f == id));
                        match known {
                            Some(&(id, count)) => ArgValue::Field(id, count),
                            // The resolver checked the field, so only a field no node of the
                            // kind can hold is missing, and it is always absent.
                            None => ArgValue::Constant(py.None()),
                        }
                    }
                };
                resolved.push(Arg { keyword, value });
            }
            Builder::Call {
                name: name.text.clone(),
                function: function(name)?,
                args: resolved,
            }
        }
    })
}

/// An int of a text, as Python's `int` reads it, without Python for a plain decimal number.
fn int<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if !digits.is_empty()
        && digits.len() <= I64_SAFE_DIGITS
        && digits.bytes().all(|b| b.is_ascii_digit())
        && let Ok(value) = text.parse::<i64>()
    {
        return Ok(value.into_pyobject(py)?.into_any());
    }
    PyInt::type_object(py).call1((crate::string(py, text)?,))
}

/// A float of a text, as Python's `float` reads it, without Python for a plain decimal number.
fn float<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    let plain = text
        .bytes()
        .all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b));
    if plain && let Ok(value) = text.parse::<f64>() {
        return Ok(PyFloat::new(py, value).into_any());
    }
    PyFloat::type_object(py).call1((crate::string(py, text)?,))
}

/// The four hexadecimal digits of a `\u` escape.
fn hex4(chars: &mut std::str::Chars<'_>) -> Option<u32> {
    let digits: String = chars.by_ref().take(4).collect();
    // from_str_radix would also take a leading sign, which JSON does not.
    (digits.len() == 4 && digits.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(&digits, 16).ok())
        .flatten()
}

/// A text without its quotes, its JSON escapes decoded and `\'` too, as `json.loads` reads a
/// string: a pair of surrogate escapes is one code point, and a lone surrogate stays one.
fn unquote<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    let mut quoted = text.chars();
    let (Some(_), Some(_)) = (quoted.next(), quoted.next_back()) else {
        let message = format!("unquote needs a text in quotes, not {text:?}");
        return Err(PyValueError::new_err(message));
    };
    let inner = quoted.as_str();
    if !inner.contains('\\') {
        return Ok(crate::string(py, inner)?.into_any());
    }
    let mut points: Vec<u32> = Vec::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            points.push(c.into());
            continue;
        }
        let escape = chars.next();
        let point = match escape {
            Some('"') => '"'.into(),
            Some('\\') => '\\'.into(),
            Some('/') => '/'.into(),
            Some('\'') => '\''.into(),
            Some('b') => 0x08,
            Some('f') => 0x0C,
            Some('n') => '\n'.into(),
            Some('r') => '\r'.into(),
            Some('t') => '\t'.into(),
            Some('u') => match hex4(&mut chars) {
                Some(high @ 0xD800..=0xDBFF) => {
                    // A high surrogate and a low one escaped after it make one code point.
                    let mut ahead = chars.clone();
                    let low = (ahead.next() == Some('\\') && ahead.next() == Some('u'))
                        .then(|| hex4(&mut ahead))
                        .flatten()
                        .filter(|low| (0xDC00..=0xDFFF).contains(low));
                    match low {
                        Some(low) => {
                            chars = ahead;
                            0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
                        }
                        None => high,
                    }
                }
                Some(unit) => unit,
                None => {
                    return Err(PyValueError::new_err(format!(
                        "unquote needs four hex digits after \\u in {text:?}"
                    )));
                }
            },
            _ => {
                let shown = escape.map_or_else(String::new, String::from);
                return Err(PyValueError::new_err(format!(
                    "unquote does not know the escape \\{shown} in {text:?}"
                )));
            }
        };
        points.push(point);
    }
    if let Some(decoded) = points
        .iter()
        .map(|&p| char::from_u32(p))
        .collect::<Option<String>>()
    {
        return Ok(crate::string(py, &decoded)?.into_any());
    }
    // A lone surrogate, which a Rust string cannot hold, goes through UCS-4.
    // SAFETY: `points` holds code points below 0x110000 for its length, which a str of kind
    // UCS-4 may hold, surrogates included.
    unsafe {
        let string = ffi::PyUnicode_FromKindAndData(
            ffi::PyUnicode_4BYTE_KIND as std::ffi::c_int,
            points.as_ptr().cast(),
            points.len() as ffi::Py_ssize_t,
        );
        Bound::from_owned_ptr_or_err(py, string)
    }
}

/// The value a builder gives a token of this text.
pub fn token_value<'py>(
    py: Python<'py>,
    builder: &Builder,
    text: &str,
) -> PyResult<Bound<'py, PyAny>> {
    match builder {
        Builder::Int => int(py, text),
        Builder::Float => float(py, text),
        Builder::Text => Ok(crate::string(py, text)?.into_any()),
        Builder::Unquote => unquote(py, text),
        Builder::Constant(value) => Ok(value.bind(py).clone()),
        Builder::Bare { function, .. } => function.bind(py).call1((crate::string(py, text)?,)),
        Builder::Call { function, args, .. } => call(py, function.bind(py), args, &[], &[]),
        // The resolver refuses these on terminals, which have no children.
        Builder::List => Ok(PyList::empty(py).into_any()),
        Builder::Tuple => Ok(PyTuple::empty(py).into_any()),
        Builder::Dict => Ok(PyDict::new(py).into_any()),
    }
}

/// The value a builder gives a node of this text and these children's values, each with the
/// field it fills.
pub fn node_value<'py>(
    py: Python<'py>,
    builder: &Builder,
    text: &str,
    children: &[(FieldId, Py<PyAny>)],
    field_names: &[Py<PyString>],
) -> PyResult<Bound<'py, PyAny>> {
    let values = || children.iter().map(|(_, value)| value.clone_ref(py));
    match builder {
        Builder::Int => int(py, text),
        Builder::Float => float(py, text),
        Builder::Text => Ok(crate::string(py, text)?.into_any()),
        Builder::Unquote => unquote(py, text),
        Builder::Constant(value) => Ok(value.bind(py).clone()),
        Builder::List => Ok(PyList::new(py, values())?.into_any()),
        Builder::Tuple => Ok(PyTuple::new(py, values())?.into_any()),
        Builder::Dict => {
            let dict = PyDict::new(py);
            for (_, value) in children {
                match value.bind(py).cast::<PyTuple>() {
                    Ok(pair) if pair.len() == 2 => {
                        dict.set_item(pair.get_item(0)?, pair.get_item(1)?)?
                    }
                    // Anything else goes through dict() for Python's own errors and pairs.
                    _ => return PyDict::type_object(py).call1((PyList::new(py, values())?,)),
                }
            }
            Ok(dict.into_any())
        }
        Builder::Bare { function, .. } => function.bind(py).call1((PyList::new(py, values())?,)),
        Builder::Call { function, args, .. } => {
            call(py, function.bind(py), args, children, field_names)
        }
    }
}

/// Call a builder's function with its arguments: fields of the node by their count, constants,
/// and the list of all children for `*`.
fn call<'py>(
    py: Python<'py>,
    function: &Bound<'py, PyAny>,
    args: &[Arg],
    children: &[(FieldId, Py<PyAny>)],
    field_names: &[Py<PyString>],
) -> PyResult<Bound<'py, PyAny>> {
    let positional = PyList::empty(py);
    let keywords = PyDict::new(py);
    for arg in args {
        let value: Py<PyAny> = match &arg.value {
            ArgValue::Constant(value) => value.clone_ref(py),
            ArgValue::Children => {
                let values = children.iter().map(|(_, value)| value.clone_ref(py));
                PyList::new(py, values)?.into_any().unbind()
            }
            ArgValue::Field(field, count) => {
                let mut items = (children.iter())
                    .filter(|(f, _)| f == field)
                    .map(|(_, value)| value.clone_ref(py));
                match count {
                    FieldCount::Many => PyList::new(py, items)?.into_any().unbind(),
                    FieldCount::One | FieldCount::Optional => {
                        let first = items.next();
                        let more = items.count();
                        if more > 0 {
                            let name = field_names.get(usize::from(field.0));
                            let message = format!(
                                "the field {} holds one item, and its children gave {} values",
                                name.map_or_else(String::new, |n| n.bind(py).to_string()),
                                more + 1
                            );
                            return Err(PyTypeError::new_err(message));
                        }
                        first.unwrap_or_else(|| py.None())
                    }
                }
            }
        };
        match &arg.keyword {
            Some(keyword) => keywords.set_item(keyword.bind(py), value)?,
            None => positional.append(value)?,
        }
    }
    let keywords = (!keywords.is_empty()).then_some(keywords);
    function.call(PyTuple::new(py, positional)?, keywords.as_ref())
}
