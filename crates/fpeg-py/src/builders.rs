// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The value builders of a grammar, `=>` after an alternative, run natively when no method of a
//! transformer handles the node or token.
//!
//! `int` and `float` take fast paths for plain ASCII numbers and Python's own conversion for
//! anything else, so their values are Python's: Rust parses decimal floats correctly rounded, as
//! CPython does. `unquote` decodes JSON's escapes as `json.loads` does, lone surrogates included.

use fpeg::{Constant, FieldId, ValueBuilder};
use pyo3::PyTypeInfo;
use pyo3::exceptions::PyValueError;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};

/// Digits an `i64` holds whatever they are, so that a number this long parses without overflow.
const I64_SAFE_DIGITS: usize = 18;

/// A built-in builder or a constant, resolved for the binding.
pub enum Builder {
    Int,
    Float,
    Text,
    Unquote,
    List,
    Tuple,
    Dict,
    Constant(Py<PyAny>),
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
        }
    }
}

/// The value of a constant written in a builder. A number with a fraction or an exponent is a
/// float, any other an int of any size.
fn constant(py: Python<'_>, value: &Constant) -> PyResult<Py<PyAny>> {
    Ok(match value {
        Constant::None => py.None(),
        Constant::Bool(value) => value.into_pyobject(py)?.to_owned().into_any().unbind(),
        Constant::Number(text) if text.contains(['.', 'e', 'E']) => {
            PyFloat::type_object(py).call1((text,))?.unbind()
        }
        Constant::Number(text) => PyInt::type_object(py).call1((text,))?.unbind(),
        Constant::String(text) => PyString::new(py, text).into_any().unbind(),
    })
}

/// Resolve a builder. A name that is no built-in names a function, which the grammar cannot be
/// given yet.
pub fn resolve(py: Python<'_>, builder: &ValueBuilder) -> PyResult<Builder> {
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
            other => return Err(function_builder(other)),
        },
        ValueBuilder::Call { name, .. } => return Err(function_builder(&name.text)),
    })
}

fn function_builder(name: &str) -> PyErr {
    PyValueError::new_err(format!(
        "the builder calls the function {name}, and no functions are given"
    ))
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
    PyInt::type_object(py).call1((text,))
}

/// A float of a text, as Python's `float` reads it, without Python for a plain decimal number.
fn float<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    let plain = text
        .bytes()
        .all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b));
    if plain && let Ok(value) = text.parse::<f64>() {
        return Ok(PyFloat::new(py, value).into_any());
    }
    PyFloat::type_object(py).call1((text,))
}

/// The four hexadecimal digits of a `\u` escape.
fn hex4(chars: &mut std::str::Chars<'_>) -> Option<u32> {
    let digits: String = chars.by_ref().take(4).collect();
    (digits.len() == 4)
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
        return Ok(PyString::new(py, inner).into_any());
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
        return Ok(PyString::new(py, &decoded).into_any());
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
        Builder::Text => Ok(PyString::new(py, text).into_any()),
        Builder::Unquote => unquote(py, text),
        Builder::Constant(value) => Ok(value.bind(py).clone()),
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
) -> PyResult<Bound<'py, PyAny>> {
    let values = || children.iter().map(|(_, value)| value.clone_ref(py));
    match builder {
        Builder::Int => int(py, text),
        Builder::Float => float(py, text),
        Builder::Text => Ok(PyString::new(py, text).into_any()),
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
    }
}
