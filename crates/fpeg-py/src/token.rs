// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Token`: a `str` subclass made natively, with its kind and positions stored inline.
//!
//! A PyO3 class cannot extend `str`, so the type comes from a spec through the C API. `str`'s own
//! `tp_new` fills the string part of every instance, which leaves the layout of `str` to CPython,
//! and the token's fields follow it: from Python 3.12 as type data (PEP 697), on 3.11 after
//! `PyUnicodeObject`. A token made so costs 38 to 61 ns, a Python class 160 to 208 (O3). Its
//! attributes are read-only, so a token is as immutable as the string it is, also across threads.

use std::ffi::{c_int, c_uint, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::{addr_of_mut, null, null_mut};
use std::sync::atomic::{AtomicIsize, AtomicPtr, Ordering};

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::panic::PanicException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString, PyTuple, PyType};

/// The positions of a token: code point offsets from 0, lines and columns from 1, the end
/// column after the last character. -1 stands for a position never given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Positions {
    pub start_pos: isize,
    pub end_pos: isize,
    pub line: isize,
    pub column: isize,
    pub end_line: isize,
    pub end_column: isize,
}

/// What a token holds after its string.
#[repr(C)]
struct TokenData {
    /// `type`: the name of its kind, an owned reference.
    kind_name: *mut ffi::PyObject,
    positions: Positions,
    /// The serial of the grammar that made it, 0 for a token made in Python.
    serial: u64,
    /// Its kind in that grammar.
    kind: u32,
}

/// The type, made once when the module loads and kept for the life of the process.
static TOKEN_TYPE: AtomicPtr<ffi::PyTypeObject> = AtomicPtr::new(null_mut());
/// Where [`TokenData`] starts in an instance of the type or of a subclass.
static DATA_OFFSET: AtomicIsize = AtomicIsize::new(0);

/// The fields of a token.
///
/// # Safety
///
/// `obj` must be an instance of the token type or of a subclass of it.
unsafe fn data(obj: *mut ffi::PyObject) -> *mut TokenData {
    // SAFETY: the caller passes a token, and every token has its data at DATA_OFFSET.
    unsafe { obj.byte_offset(DATA_OFFSET.load(Ordering::Relaxed)).cast() }
}

/// An instance of a `str` subtype made by `str`'s own `tp_new`, as `subtype(value)` would make
/// it, with its token fields zeroed.
fn new_str_subtype<'py>(
    py: Python<'py>,
    subtype: *mut ffi::PyTypeObject,
    value: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let args = PyTuple::new(py, [value])?;
    // SAFETY: `str`'s tp_new takes any subtype of str, and `args` is a tuple of one object.
    let obj = unsafe {
        let new = (*addr_of_mut!(ffi::PyUnicode_Type))
            .tp_new
            .ok_or_else(|| PyTypeError::new_err("str has no constructor"))?;
        new(subtype, args.as_ptr(), null_mut())
    };
    // SAFETY: tp_new returns a new reference or null with an exception set.
    unsafe { Bound::from_owned_ptr_or_err(py, obj) }
}

/// Give a token made by [`new_str_subtype`] its fields.
fn fill(
    obj: &Bound<'_, PyAny>,
    kind_name: Bound<'_, PyAny>,
    positions: Positions,
    serial: u64,
    kind: u32,
) {
    // SAFETY: `obj` is a fresh token no other code has seen, so writing its fields races with
    // nothing. The kind name's reference moves into the token.
    unsafe {
        let d = data(obj.as_ptr());
        (*d).kind_name = kind_name.into_ptr();
        (*d).positions = positions;
        (*d).serial = serial;
        (*d).kind = kind;
    }
}

/// A token of `text`, made natively.
pub fn make<'py>(
    py: Python<'py>,
    text: &str,
    kind_name: &Bound<'py, PyString>,
    positions: Positions,
    serial: u64,
    kind: u32,
) -> PyResult<Bound<'py, PyAny>> {
    let ty = TOKEN_TYPE.load(Ordering::Relaxed);
    let obj = new_str_subtype(py, ty, PyString::new(py, text).into_any())?;
    fill(&obj, kind_name.clone().into_any(), positions, serial, kind);
    Ok(obj)
}

/// The positions of a token.
fn positions_of(obj: &Bound<'_, PyAny>) -> Positions {
    // SAFETY: callers pass tokens, whose positions never change after they are made.
    unsafe { (*data(obj.as_ptr())).positions }
}

/// The kind name of a token.
fn kind_name_of<'py>(obj: &Bound<'py, PyAny>) -> Bound<'py, PyAny> {
    // SAFETY: callers pass tokens, and a token's kind name is an object it owns, set when it is
    // made and never changed.
    unsafe {
        let kind = (*data(obj.as_ptr())).kind_name;
        if kind.is_null() {
            obj.py().None().into_bound(obj.py())
        } else {
            Bound::from_borrowed_ptr(obj.py(), kind)
        }
    }
}

/// The text of a token as a plain `str`.
fn plain<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    // SAFETY: callers pass tokens, which are strs, and PyUnicode_FromObject copies a str subclass
    // into a plain str or returns null with an exception set.
    unsafe { Bound::from_owned_ptr_or_err(obj.py(), ffi::PyUnicode_FromObject(obj.as_ptr())) }
}

/// A position as Python sees it.
fn position_object(py: Python<'_>, value: isize) -> Py<PyAny> {
    if value < 0 {
        py.None()
    } else {
        value
            .into_pyobject(py)
            .map_or_else(|_| py.None(), |v| v.into_any().unbind())
    }
}

/// A position given from Python: an int, or `None` for none.
fn position_from(value: Option<&Bound<'_, PyAny>>) -> PyResult<isize> {
    match value {
        None => Ok(-1),
        Some(value) if value.is_none() => Ok(-1),
        Some(value) => {
            let position: isize = value.extract()?;
            if position < 0 {
                return Err(PyValueError::new_err("a position is never negative"));
            }
            Ok(position)
        }
    }
}

/// Run the body of a slot or method of the type, turning an error or a panic into a Python
/// exception, as CPython expects of a C function: null with the exception set.
fn trap(body: impl FnOnce(Python<'_>) -> PyResult<*mut ffi::PyObject>) -> *mut ffi::PyObject {
    // SAFETY: CPython calls the slots and methods of a type with the thread attached.
    let py = unsafe { Python::assume_attached() };
    match catch_unwind(AssertUnwindSafe(|| body(py))) {
        Ok(Ok(obj)) => obj,
        Ok(Err(error)) => {
            error.restore(py);
            null_mut()
        }
        Err(_) => {
            PanicException::new_err("a bug in fpeg's Token").restore(py);
            null_mut()
        }
    }
}

/// The arguments of a call, matched to `names` by position and keyword, the first `required`
/// given.
fn bind<'py>(
    args: &Bound<'py, PyTuple>,
    kwargs: Option<&Bound<'py, PyDict>>,
    names: &[&str],
    required: usize,
    what: &str,
) -> PyResult<Vec<Option<Bound<'py, PyAny>>>> {
    if args.len() > names.len() {
        let message = format!("{what} takes at most {} arguments", names.len());
        return Err(PyTypeError::new_err(message));
    }
    let mut values: Vec<Option<Bound<'py, PyAny>>> = vec![None; names.len()];
    for (slot, arg) in values.iter_mut().zip(args.iter()) {
        *slot = Some(arg);
    }
    for (key, value) in kwargs.into_iter().flat_map(|kwargs| kwargs.iter()) {
        let key: String = key.extract()?;
        let Some(index) = names.iter().position(|name| *name == key) else {
            return Err(PyTypeError::new_err(format!(
                "{what} has no argument {key}"
            )));
        };
        if values[index].is_some() {
            return Err(PyTypeError::new_err(format!("{what} got {key} twice")));
        }
        values[index] = Some(value);
    }
    if let Some(missing) = (0..required).find(|&i| values[i].is_none()) {
        let message = format!("{what} needs the argument {}", names[missing]);
        return Err(PyTypeError::new_err(message));
    }
    Ok(values)
}

/// The arguments of `Token(...)`, in Lark's order.
const CONSTRUCTOR: [&str; 8] = [
    "type",
    "value",
    "start_pos",
    "line",
    "column",
    "end_line",
    "end_column",
    "end_pos",
];

/// A token of a type and value with the given positions, as `Token(...)` makes it.
fn construct<'py>(
    py: Python<'py>,
    subtype: *mut ffi::PyTypeObject,
    values: &[Option<Bound<'py, PyAny>>],
) -> PyResult<Bound<'py, PyAny>> {
    let value = |i: usize| values.get(i).and_then(Option::as_ref);
    let positions = Positions {
        start_pos: position_from(value(2))?,
        line: position_from(value(3))?,
        column: position_from(value(4))?,
        end_line: position_from(value(5))?,
        end_column: position_from(value(6))?,
        end_pos: position_from(value(7))?,
    };
    let kind_name = value(0).map_or_else(|| py.None().into_bound(py), Clone::clone);
    let text = value(1).map_or_else(|| PyString::new(py, "").into_any(), Clone::clone);
    let obj = new_str_subtype(py, subtype, text)?;
    fill(&obj, kind_name, positions, 0, 0);
    Ok(obj)
}

unsafe extern "C" fn token_new(
    subtype: *mut ffi::PyTypeObject,
    args: *mut ffi::PyObject,
    kwargs: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    trap(|py| {
        // SAFETY: CPython passes a tuple of arguments and a dict of keywords or null.
        let (args, kwargs) = unsafe {
            let args = Bound::from_borrowed_ptr(py, args).cast_into::<PyTuple>()?;
            let kwargs = if kwargs.is_null() {
                None
            } else {
                Some(Bound::from_borrowed_ptr(py, kwargs).cast_into::<PyDict>()?)
            };
            (args, kwargs)
        };
        let values = bind(&args, kwargs.as_ref(), &CONSTRUCTOR, 2, "Token()")?;
        Ok(construct(py, subtype, &values)?.into_ptr())
    })
}

unsafe extern "C" fn token_dealloc(obj: *mut ffi::PyObject) {
    // SAFETY: CPython deallocates a token once, with no reference left. The base dealloc frees
    // the string and the object, and a heap type's instances own a reference to their type.
    unsafe {
        let d = data(obj);
        ffi::Py_XDECREF((*d).kind_name);
        let ty = ffi::Py_TYPE(obj);
        if let Some(base) = (*addr_of_mut!(ffi::PyUnicode_Type)).tp_dealloc {
            base(obj);
        }
        ffi::Py_DECREF(ty.cast());
    }
}

unsafe extern "C" fn token_repr(obj: *mut ffi::PyObject) -> *mut ffi::PyObject {
    trap(|py| {
        // SAFETY: CPython calls tp_repr with a live token.
        let obj = unsafe { Bound::from_borrowed_ptr(py, obj) };
        let repr = format!(
            "Token({}, {})",
            kind_name_of(&obj).repr()?,
            plain(&obj)?.repr()?
        );
        Ok(PyString::new(py, &repr).into_ptr())
    })
}

unsafe extern "C" fn get_type(obj: *mut ffi::PyObject, _: *mut c_void) -> *mut ffi::PyObject {
    trap(|py| {
        // SAFETY: CPython calls a getter of the type with a live token.
        let obj = unsafe { Bound::from_borrowed_ptr(py, obj) };
        Ok(kind_name_of(&obj).into_ptr())
    })
}

unsafe extern "C" fn get_value(obj: *mut ffi::PyObject, _: *mut c_void) -> *mut ffi::PyObject {
    // SAFETY: a getter gets a live token, a str, and PyUnicode_FromObject copies a str subclass
    // into a plain str or returns null with an exception set.
    unsafe { ffi::PyUnicode_FromObject(obj) }
}

unsafe extern "C" fn get_position(
    obj: *mut ffi::PyObject,
    field: *mut c_void,
) -> *mut ffi::PyObject {
    trap(|py| {
        // SAFETY: CPython calls a getter of the type with a live token.
        let obj = unsafe { Bound::from_borrowed_ptr(py, obj) };
        let p = positions_of(&obj);
        let value = match field.addr() {
            0 => p.start_pos,
            1 => p.end_pos,
            2 => p.line,
            3 => p.column,
            4 => p.end_line,
            _ => p.end_column,
        };
        Ok(position_object(py, value).into_ptr())
    })
}

/// `update(type=None, value=None)`: a token like this one with another type or value.
unsafe extern "C" fn token_update(
    obj: *mut ffi::PyObject,
    args: *mut ffi::PyObject,
    kwargs: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    trap(|py| {
        // SAFETY: CPython passes the token, a tuple of arguments and a dict of keywords or null.
        let (obj, args, kwargs) = unsafe {
            let obj = Bound::from_borrowed_ptr(py, obj);
            let args = Bound::from_borrowed_ptr(py, args).cast_into::<PyTuple>()?;
            let kwargs = if kwargs.is_null() {
                None
            } else {
                Some(Bound::from_borrowed_ptr(py, kwargs).cast_into::<PyDict>()?)
            };
            (obj, args, kwargs)
        };
        let values = bind(&args, kwargs.as_ref(), &["type", "value"], 0, "update()")?;
        let given = |i: usize| values[i].clone().filter(|v| !v.is_none());
        let kind_name = given(0).unwrap_or_else(|| kind_name_of(&obj));
        let text = match given(1) {
            Some(text) => text,
            None => plain(&obj)?,
        };
        let ty = TOKEN_TYPE.load(Ordering::Relaxed);
        let new = new_str_subtype(py, ty, text)?;
        fill(&new, kind_name, positions_of(&obj), 0, 0);
        Ok(new.into_ptr())
    })
}

/// `Token.new_borrow_pos(type, value, borrow_t)`: a token with the positions of another.
unsafe extern "C" fn token_new_borrow_pos(
    cls: *mut ffi::PyObject,
    args: *mut ffi::PyObject,
    kwargs: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    trap(|py| {
        // SAFETY: CPython passes the class, a tuple of arguments and a dict of keywords or null.
        let (cls, args, kwargs) = unsafe {
            let cls = Bound::from_borrowed_ptr(py, cls).cast_into::<PyType>()?;
            let args = Bound::from_borrowed_ptr(py, args).cast_into::<PyTuple>()?;
            let kwargs = if kwargs.is_null() {
                None
            } else {
                Some(Bound::from_borrowed_ptr(py, kwargs).cast_into::<PyDict>()?)
            };
            (cls, args, kwargs)
        };
        let names = ["type", "value", "borrow_t"];
        let values = bind(&args, kwargs.as_ref(), &names, 3, "new_borrow_pos()")?;
        let borrowed = values[2].as_ref().map(|t| {
            let attribute = |name: &str| t.getattr(name).ok().filter(|v| !v.is_none());
            [
                attribute("start_pos"),
                attribute("line"),
                attribute("column"),
                attribute("end_line"),
                attribute("end_column"),
                attribute("end_pos"),
            ]
        });
        let mut all = vec![values[0].clone(), values[1].clone()];
        all.extend(borrowed.into_iter().flatten());
        Ok(construct(py, cls.as_type_ptr(), &all)?.into_ptr())
    })
}

/// `__reduce__`: the token as `Token(...)` would make it again, without its grammar.
unsafe extern "C" fn token_reduce(
    obj: *mut ffi::PyObject,
    _: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    trap(|py| {
        // SAFETY: CPython passes the token.
        let obj = unsafe { Bound::from_borrowed_ptr(py, obj) };
        let p = positions_of(&obj);
        let text = plain(&obj)?;
        let args = PyTuple::new(
            py,
            [
                kind_name_of(&obj).unbind(),
                text.unbind(),
                position_object(py, p.start_pos),
                position_object(py, p.line),
                position_object(py, p.column),
                position_object(py, p.end_line),
                position_object(py, p.end_column),
                position_object(py, p.end_pos),
            ],
        )?;
        let reduced = PyTuple::new(py, [obj.get_type().into_any(), args.into_any()])?;
        Ok(reduced.into_ptr())
    })
}

// The docstrings of the type and its methods. CPython reads a signature from the start of a
// docstring: the name with its arguments, then a line "--".
const TOKEN_DOC: &std::ffi::CStr = c"Token(type, value, start_pos=None, line=None, column=None, end_line=None, end_column=None, end_pos=None)\n--\n\nA token of a parse: a str with the name of its kind and its place, as Lark's Token. Positions count code points, lines and columns count from 1.";
const UPDATE_DOC: &std::ffi::CStr = c"update($self, /, type=None, value=None)\n--\n\nA token like this one with another type or value, at the same place.";
const NEW_BORROW_POS_DOC: &std::ffi::CStr = c"new_borrow_pos($type, /, type, value, borrow_t)\n--\n\nA token of a type and value at the place of another token.";

/// Make the token type. Called once, when the module loads.
pub fn create_type(py: Python<'_>) -> PyResult<Bound<'_, PyType>> {
    let getset = |name: &'static std::ffi::CStr,
                  get: ffi::getter,
                  doc: &'static std::ffi::CStr,
                  field: usize| {
        ffi::PyGetSetDef {
            name: name.as_ptr(),
            get: Some(get),
            set: None,
            doc: doc.as_ptr(),
            closure: std::ptr::without_provenance_mut(field),
        }
    };
    let getsets = Box::leak(Box::new([
        getset(c"type", get_type, c"The name of the token's kind.", 0),
        getset(c"value", get_value, c"The text as a plain str.", 0),
        getset(
            c"start_pos",
            get_position,
            c"Code points before the token.",
            0,
        ),
        getset(c"end_pos", get_position, c"Code points before its end.", 1),
        getset(c"line", get_position, c"The line it starts on, from 1.", 2),
        getset(
            c"column",
            get_position,
            c"The column it starts at, from 1.",
            3,
        ),
        getset(
            c"end_line",
            get_position,
            c"The line it ends on, from 1.",
            4,
        ),
        getset(
            c"end_column",
            get_position,
            c"The column after its last character.",
            5,
        ),
        ffi::PyGetSetDef::default(),
    ]));
    let methods = Box::leak(Box::new([
        ffi::PyMethodDef {
            ml_name: c"update".as_ptr(),
            ml_meth: ffi::PyMethodDefPointer {
                PyCFunctionWithKeywords: token_update,
            },
            ml_flags: ffi::METH_VARARGS | ffi::METH_KEYWORDS,
            ml_doc: UPDATE_DOC.as_ptr(),
        },
        ffi::PyMethodDef {
            ml_name: c"new_borrow_pos".as_ptr(),
            ml_meth: ffi::PyMethodDefPointer {
                PyCFunctionWithKeywords: token_new_borrow_pos,
            },
            ml_flags: ffi::METH_VARARGS | ffi::METH_KEYWORDS | ffi::METH_CLASS,
            ml_doc: NEW_BORROW_POS_DOC.as_ptr(),
        },
        ffi::PyMethodDef {
            ml_name: c"__reduce__".as_ptr(),
            ml_meth: ffi::PyMethodDefPointer {
                PyCFunction: token_reduce,
            },
            ml_flags: ffi::METH_NOARGS,
            ml_doc: null(),
        },
        ffi::PyMethodDef::zeroed(),
    ]));
    let slots = Box::leak(Box::new([
        ffi::PyType_Slot {
            slot: ffi::Py_tp_new,
            pfunc: token_new as *mut c_void,
        },
        ffi::PyType_Slot {
            slot: ffi::Py_tp_dealloc,
            pfunc: token_dealloc as *mut c_void,
        },
        ffi::PyType_Slot {
            slot: ffi::Py_tp_repr,
            pfunc: token_repr as *mut c_void,
        },
        ffi::PyType_Slot {
            slot: ffi::Py_tp_getset,
            pfunc: getsets.as_mut_ptr().cast(),
        },
        ffi::PyType_Slot {
            slot: ffi::Py_tp_methods,
            pfunc: methods.as_mut_ptr().cast(),
        },
        ffi::PyType_Slot {
            slot: ffi::Py_tp_doc,
            pfunc: TOKEN_DOC.as_ptr().cast_mut().cast(),
        },
        ffi::PyType_Slot {
            slot: 0,
            pfunc: null_mut(),
        },
    ]));
    // From 3.12 a negative size asks for type data after the base (PEP 697). Before it, the
    // fields follow PyUnicodeObject, whose size the headers of the version give.
    #[cfg(Py_3_12)]
    let basicsize = -(std::mem::size_of::<TokenData>() as c_int);
    #[cfg(not(Py_3_12))]
    let basicsize =
        (std::mem::size_of::<ffi::PyUnicodeObject>() + std::mem::size_of::<TokenData>()) as c_int;
    let spec = Box::leak(Box::new(ffi::PyType_Spec {
        name: c"fpeg.Token".as_ptr(),
        basicsize,
        itemsize: 0,
        flags: (ffi::Py_TPFLAGS_DEFAULT | ffi::Py_TPFLAGS_BASETYPE | ffi::Py_TPFLAGS_IMMUTABLETYPE)
            as c_uint,
        slots: slots.as_mut_ptr(),
    }));
    // SAFETY: the spec and its tables are leaked, so they outlive the type, as CPython needs.
    // `bases` is a new tuple of the str type.
    let ty = unsafe {
        let bases = ffi::PyTuple_Pack(1, addr_of_mut!(ffi::PyUnicode_Type).cast::<ffi::PyObject>());
        let bases = Bound::from_owned_ptr_or_err(py, bases)?;
        let ty = ffi::PyType_FromSpecWithBases(spec, bases.as_ptr());
        Bound::from_owned_ptr_or_err(py, ty)?.cast_into::<PyType>()?
    };
    TOKEN_TYPE.store(ty.as_type_ptr(), Ordering::Relaxed);
    // Find where the fields start from one instance, whose fields are still zero.
    let sample = new_str_subtype(py, ty.as_type_ptr(), PyString::new(py, "x").into_any())?;
    #[cfg(Py_3_12)]
    // SAFETY: `sample` is an instance of `ty`, whose type data PEP 697 places.
    let at = unsafe { ffi::PyObject_GetTypeData(sample.as_ptr(), ty.as_type_ptr()) };
    #[cfg(not(Py_3_12))]
    // SAFETY: on 3.11 the fields follow the PyUnicodeObject that starts the instance.
    let at = unsafe {
        sample
            .as_ptr()
            .byte_add(std::mem::size_of::<ffi::PyUnicodeObject>())
            .cast::<c_void>()
    };
    DATA_OFFSET.store(
        at.addr() as isize - sample.as_ptr().addr() as isize,
        Ordering::Relaxed,
    );
    Ok(ty)
}
