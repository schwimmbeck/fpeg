// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The native transform: one forward scan over the entries of a subtree in postorder, with a
//! stack of values, following the plan that `fpeg._transformer` makes once per class and grammar.
//!
//! A node takes the values above the place on the stack where its subtree began, which are its
//! visible children: deeper entries were taken by their own parents, and hidden tokens push
//! nothing. An entry that no method reaches pushes no Python object, and a node whose children
//! are all such entries stays one, so a subtree without methods costs no object at all.

use std::ptr::null_mut;
use std::sync::{Arc, Mutex, PoisonError};

use fpeg::{LineCursor, NodeRef};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyString, PyTuple, PyType};

use crate::grammar::GrammarCore;
use crate::token;
use crate::tree::{Meta, Parsed, Tree, item, token_positions};

/// Entries the scan goes through between two looks for a signal such as Ctrl+C.
const SIGNAL_CHECK_ENTRIES: usize = 65_536;

/// How a plan calls a method. The codes are those of `fpeg._transformer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    List,
    Inline,
    Nothing,
    Token,
}

impl Mode {
    fn from_code(code: u8) -> PyResult<Mode> {
        Ok(match code {
            1 => Mode::List,
            2 => Mode::Inline,
            4 => Mode::Nothing,
            8 => Mode::Token,
            _ => return Err(PyValueError::new_err(format!("no calling mode {code}"))),
        })
    }
}

/// How the method of a kind is called.
struct Handler {
    name: Py<PyString>,
    mode: Mode,
}

/// How a transformer class handles the kinds of a grammar.
pub struct Plan {
    /// By kind id.
    handlers: Vec<Option<Handler>>,
    /// Whether the class overrides `__default__` and `__default_token__`.
    default_node: bool,
    default_token: bool,
}

/// The plans of a grammar, one per transformer class, kept as long as the grammar.
#[derive(Default)]
pub struct Plans(Mutex<Vec<(Py<PyType>, Arc<Plan>)>>);

impl Plans {
    fn find(&self, class: &Bound<'_, PyType>) -> Option<Arc<Plan>> {
        let plans = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        (plans.iter())
            .find(|(known, _)| known.is(class))
            .map(|(_, plan)| Arc::clone(plan))
    }

    fn insert(&self, class: &Bound<'_, PyType>, plan: Arc<Plan>) -> Arc<Plan> {
        let mut plans = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        // Another thread may have made the same plan meanwhile, and then its plan is kept.
        if let Some((_, known)) = plans.iter().find(|(known, _)| known.is(class)) {
            return Arc::clone(known);
        }
        plans.push((class.clone().unbind(), Arc::clone(&plan)));
        plan
    }
}

/// The kinds of a grammar as the plan maker reads them: name, and whether they are tokens.
fn kind_table<'py>(py: Python<'py>, core: &GrammarCore) -> PyResult<Bound<'py, PyList>> {
    let mut rows = Vec::new();
    for (id, kind) in core.parser.kinds().iter().enumerate() {
        let row = (core.kind_names[id].clone_ref(py), kind.token);
        rows.push(row.into_pyobject(py)?);
    }
    PyList::new(py, rows)
}

/// The plan of a class for a grammar, made by `make_plan` the first time they meet.
fn plan_for(
    py: Python<'_>,
    core: &GrammarCore,
    class: &Bound<'_, PyType>,
    make_plan: &Bound<'_, PyAny>,
) -> PyResult<Arc<Plan>> {
    if let Some(plan) = core.plans.find(class) {
        return Ok(plan);
    }
    type Entry = (usize, String, u8);
    let made = make_plan.call1((class, kind_table(py, core)?))?;
    let (entries, default_node, default_token): (Vec<Entry>, bool, bool) = made.extract()?;
    let mut handlers: Vec<Option<Handler>> = (0..core.kind_names.len()).map(|_| None).collect();
    for (kind, name, mode) in entries {
        if let Some(slot) = handlers.get_mut(kind) {
            *slot = Some(Handler {
                name: PyString::intern(py, &name).unbind(),
                mode: Mode::from_code(mode)?,
            });
        }
    }
    let plan = Arc::new(Plan {
        handlers,
        default_node,
        default_token,
    });
    Ok(core.plans.insert(class, plan))
}

/// A value on the stack: the entry it came from, and the object, or none for an entry that no
/// method reached.
struct Value {
    entry: u32,
    object: Option<Py<PyAny>>,
}

struct Scan<'py, 'p> {
    py: Python<'py>,
    transformer: &'p Bound<'py, PyAny>,
    parsed: &'p Arc<Parsed>,
    plan: &'p Plan,
    visit_tokens: bool,
    /// The bound methods met in this transform, by kind, found at the first entry of each.
    methods: Vec<Option<Py<PyAny>>>,
    stack: Vec<Value>,
    /// Positions of the tokens the scan makes, which come in input order.
    cursor: Option<LineCursor<'p>>,
}

impl<'py, 'p> Scan<'py, 'p> {
    fn run(&mut self, root: usize) -> PyResult<()> {
        let tree = &self.parsed.tree;
        let size = tree.node(root).map_or(1, |node| node.size());
        for index in root + 1 - size..=root {
            if index % SIGNAL_CHECK_ENTRIES == 0 {
                self.py.check_signals()?;
            }
            let Some(node) = tree.node(index) else { break };
            if node.is_token() {
                if !node.is_hidden() {
                    self.token(node)?;
                }
            } else {
                self.node(node)?;
            }
        }
        Ok(())
    }

    fn method(&mut self, kind: usize, name: &Py<PyString>) -> PyResult<Bound<'py, PyAny>> {
        if let Some(Some(method)) = self.methods.get(kind) {
            return Ok(method.bind(self.py).clone());
        }
        let method = self.transformer.getattr(name.bind(self.py))?;
        if let Some(slot) = self.methods.get_mut(kind) {
            *slot = Some(method.clone().unbind());
        }
        Ok(method)
    }

    /// The token of an entry, made now with positions from the cursor.
    fn make_token(&mut self, node: NodeRef<'_>) -> PyResult<Bound<'py, PyAny>> {
        let parsed = self.parsed;
        let (start, end) = node.span();
        let cursor =
            (self.cursor).get_or_insert_with(|| LineCursor::new(parsed.lines(), &parsed.source));
        let positions = token_positions(
            cursor.position(start as usize),
            cursor.position(end as usize),
        );
        let kind = node.kind().0;
        let name = parsed.grammar.kind_names[kind as usize].bind(self.py);
        let text = parsed.text(start, end);
        token::make(self.py, text, name, positions, parsed.grammar.serial, kind)
    }

    fn object(&self, value: Value) -> PyResult<Py<PyAny>> {
        match value.object {
            Some(object) => Ok(object),
            None => item(self.py, self.parsed, value.entry as usize),
        }
    }

    fn objects(&self, values: Vec<Value>) -> PyResult<Vec<Py<PyAny>>> {
        values.into_iter().map(|value| self.object(value)).collect()
    }

    fn meta(&self, node: NodeRef<'_>) -> PyResult<Py<PyAny>> {
        let (start, end) = node.span();
        Ok(Py::new(self.py, Meta::of(self.parsed, start, end))?.into_any())
    }

    /// Push what a method returned.
    fn push(&mut self, entry: usize, result: Bound<'py, PyAny>) -> PyResult<()> {
        self.stack.push(Value {
            entry: entry as u32,
            object: Some(result.unbind()),
        });
        Ok(())
    }

    fn token(&mut self, node: NodeRef<'_>) -> PyResult<()> {
        let index = node.index();
        let kind = node.kind().0 as usize;
        let handler = (self.plan.handlers.get(kind)).and_then(Option::as_ref);
        if self.visit_tokens
            && let Some(handler) = handler
        {
            let (name, mode) = (handler.name.clone_ref(self.py), handler.mode);
            let method = self.method(kind, &name)?;
            let token = self.make_token(node)?;
            let result = match mode {
                Mode::Nothing => method.call0(),
                _ => method.call1((token,)),
            };
            let result = result?;
            return self.push(index, result);
        }
        if self.plan.default_token {
            let token = self.make_token(node)?;
            let result =
                (self.transformer).call_method1(intern!(self.py, "__default_token__"), (token,))?;
            return self.push(index, result);
        }
        self.stack.push(Value {
            entry: index as u32,
            object: None,
        });
        Ok(())
    }

    fn node(&mut self, node: NodeRef<'_>) -> PyResult<()> {
        let index = node.index();
        let first = index + 1 - node.size();
        let base = (self.stack.iter())
            .rposition(|value| (value.entry as usize) < first)
            .map_or(0, |at| at + 1);
        let children = self.stack.split_off(base);
        let kind = node.kind().0 as usize;
        let handler = (self.plan.handlers.get(kind)).and_then(Option::as_ref);
        if let Some(handler) = handler {
            let result = self.call(kind, handler, children)?;
            return self.push(index, result);
        }
        let data = self.parsed.grammar.kind_names[kind].clone_ref(self.py);
        if self.plan.default_node {
            let children = PyList::new(self.py, self.objects(children)?)?;
            let meta = self.meta(node)?;
            let result = (self.transformer)
                .call_method1(intern!(self.py, "__default__"), (data, children, meta))?;
            return self.push(index, result);
        }
        if children.iter().all(|child| child.object.is_none()) {
            self.stack.push(Value {
                entry: index as u32,
                object: None,
            });
            return Ok(());
        }
        let children = PyList::new(self.py, self.objects(children)?)?;
        let origin = Some((Arc::clone(self.parsed), index as u32));
        let tree = Tree::owned(data.into_any(), children.unbind(), origin);
        self.stack.push(Value {
            entry: index as u32,
            object: Some(Py::new(self.py, tree)?.into_any()),
        });
        Ok(())
    }

    /// Call the method of a node as its plan says.
    fn call(
        &mut self,
        kind: usize,
        handler: &Handler,
        children: Vec<Value>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = self.py;
        let name = handler.name.clone_ref(py);
        let method = self.method(kind, &name)?;
        let objects = self.objects(children)?;
        let args: Vec<Py<PyAny>> = match handler.mode {
            Mode::List => vec![PyList::new(py, objects)?.into_any().unbind()],
            Mode::Inline => objects,
            Mode::Nothing | Mode::Token => Vec::new(),
        };
        vectorcall(&method, &args, &[])
    }
}

/// Call `callable` with positional and keyword arguments through vectorcall.
fn vectorcall<'py>(
    callable: &Bound<'py, PyAny>,
    args: &[Py<PyAny>],
    keywords: &[(&'static str, Py<PyAny>)],
) -> PyResult<Bound<'py, PyAny>> {
    let py = callable.py();
    // A free slot before the arguments lets a bound method put `self` there without copying,
    // which PY_VECTORCALL_ARGUMENTS_OFFSET allows.
    let mut pointers: Vec<*mut ffi::PyObject> = Vec::with_capacity(1 + args.len() + keywords.len());
    pointers.push(null_mut());
    pointers.extend(args.iter().map(Py::as_ptr));
    pointers.extend(keywords.iter().map(|(_, value)| value.as_ptr()));
    let names = if keywords.is_empty() {
        None
    } else {
        let names = keywords.iter().map(|(name, _)| PyString::intern(py, name));
        Some(PyTuple::new(py, names)?)
    };
    let names_pointer = names.as_ref().map_or(null_mut(), |n| n.as_ptr());
    // SAFETY: the pointers borrow from `args` and `keywords`, which outlive the call. The count
    // leaves out the free slot and the keyword values, whose names `names` holds.
    let result = unsafe {
        ffi::PyObject_Vectorcall(
            callable.as_ptr(),
            pointers.as_ptr().add(1),
            args.len() | ffi::PY_VECTORCALL_ARGUMENTS_OFFSET,
            names_pointer,
        )
    };
    // SAFETY: vectorcall returns a new reference, or null with an exception set.
    unsafe { Bound::from_owned_ptr_or_err(py, result) }
}

/// Transform a view with the plan of the transformer's class, and return the value its node
/// gives its parent, in a tuple.
#[pyfunction]
pub fn scan<'py>(
    py: Python<'py>,
    transformer: &Bound<'py, PyAny>,
    view: &Bound<'py, Tree>,
    make_plan: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyTuple>> {
    let Some((parsed, index)) = view.get().parse_entry() else {
        return Err(PyTypeError::new_err(
            "only a tree of a parse can be scanned",
        ));
    };
    let plan = plan_for(py, &parsed.grammar, &transformer.get_type(), make_plan)?;
    let visit_tokens = match transformer.getattr_opt(intern!(py, "__visit_tokens__"))? {
        Some(value) => value.is_truthy()?,
        None => true,
    };
    let mut scan = Scan {
        py,
        transformer,
        parsed,
        plan: &plan,
        visit_tokens,
        methods: (0..parsed.grammar.kind_names.len()).map(|_| None).collect(),
        stack: Vec::new(),
        cursor: None,
    };
    scan.run(index as usize)?;
    let values = std::mem::take(&mut scan.stack);
    // A node that nothing changed gives back the view it came as.
    if let [value] = values.as_slice()
        && value.object.is_none()
        && value.entry == index
    {
        return PyTuple::new(py, [view.clone().into_any()]);
    }
    PyTuple::new(py, scan.objects(values)?)
}
