// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The native transform: one forward scan over the entries of a subtree in postorder, with a
//! stack of values, following the plan that `fpeg._transformer` makes once per class and grammar.
//!
//! A node takes the values above the place on the stack where its subtree began, which are its
//! visible children: deeper entries were taken by their own parents, and hidden tokens push
//! nothing. An entry that no method reaches pushes no Python object, and a node whose children
//! are all such entries stays one, so a subtree without methods costs no object at all.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex, PoisonError};

use fpeg::{FieldCount, FieldId, LineCursor, NodeRef};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyList, PyString, PyTuple, PyType, PyWeakrefReference};
use pyo3::{PyTypeInfo, intern};

use crate::builders;
use crate::grammar::{GrammarCore, grammar_of};
use crate::token;
use crate::tree::{Meta, Parsed, Tree, item, token_positions};

/// Entries the scan goes through between two looks for a signal such as Ctrl+C.
const SIGNAL_CHECK_ENTRIES: usize = 65_536;

/// Pointers a call passes from the stack: the free slot, the arguments and the keyword values.
/// A call with more puts them on the heap.
const CALL_STACK_POINTERS: usize = 8;

/// How a plan calls a method. The codes are those of `fpeg._transformer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    List,
    Inline,
    Fields,
    Nothing,
    Meta,
    MetaInline,
    Tree,
    Token,
}

impl Mode {
    fn from_code(code: u8) -> PyResult<Mode> {
        Ok(match code {
            1 => Mode::List,
            2 => Mode::Inline,
            3 => Mode::Fields,
            4 => Mode::Nothing,
            5 => Mode::Meta,
            6 => Mode::MetaInline,
            7 => Mode::Tree,
            8 => Mode::Token,
            _ => return Err(PyValueError::new_err(format!("no calling mode {code}"))),
        })
    }
}

/// The keyword-only extras of a method, as bits, as `fpeg._transformer` gives them.
const EXTRA_META: u8 = 1;
const EXTRA_NODE: u8 = 2;
const EXTRA_CHILDREN: u8 = 4;

/// How the method of a kind is called.
struct Handler {
    name: Py<PyString>,
    mode: Mode,
    /// The fields of a method that takes fields, in its order, with how many items each holds.
    fields: Vec<(FieldId, FieldCount)>,
    extras: u8,
}

/// How a transformer class handles the kinds of a grammar.
pub struct Plan {
    /// By kind id.
    handlers: Vec<Option<Handler>>,
    /// Whether the class overrides `__default__` and `__default_token__`.
    default_node: bool,
    default_token: bool,
    /// The positions the values get, if the class asks for them.
    positions: Option<Stamp>,
}

/// How columns count, by the codes of `fpeg._transformer`.
#[derive(Clone, Copy)]
enum Unit {
    CodePoints,
    Utf8,
    Utf16,
}

/// The positions a transformer class gives its values: the attributes of the start line and
/// column and of the end line and column, and how lines and columns count. The classes whose
/// instances get them stay with the class, since a plan that held them would keep a class
/// that refers back to the transformer alive as long as the grammar.
struct Stamp {
    names: [Py<PyString>; 4],
    line_base: i64,
    column_base: i64,
    unit: Unit,
}

/// The positions of a plan as `fpeg._transformer` gives them.
type StampEntry = (String, String, String, String, i64, i64, u8);

impl Stamp {
    fn from_entry(py: Python<'_>, entry: StampEntry) -> PyResult<Stamp> {
        let (start_line, start_column, end_line, end_column, line_base, column_base, unit) = entry;
        let unit = match unit {
            0 => Unit::CodePoints,
            1 => Unit::Utf8,
            2 => Unit::Utf16,
            _ => return Err(PyValueError::new_err(format!("no column unit {unit}"))),
        };
        let name = |text: &str| PyString::intern(py, text).unbind();
        Ok(Stamp {
            names: [
                name(&start_line),
                name(&start_column),
                name(&end_line),
                name(&end_column),
            ],
            line_base,
            column_base,
            unit,
        })
    }
}

/// The plans of a grammar, one per class, by the address of the class. The classes are held
/// weakly, so that a class made for one task goes when it is no longer used, and its plan with
/// the next insert. A walk asks at every node, so the plan is found without a search.
pub struct Plans<P>(Mutex<HashMap<usize, Kept<P>>>);

/// The plan of a class, with a weak reference to the class that tells whether it is still
/// the one at its address.
struct Kept<P> {
    class: Py<PyWeakrefReference>,
    plan: Arc<P>,
}

impl<P> Default for Plans<P> {
    fn default() -> Plans<P> {
        Plans(Mutex::new(HashMap::new()))
    }
}

/// The key of a class among the plans. A class that has gone may leave its address to another,
/// which the weak reference of the plan tells apart.
fn address(class: &Bound<'_, PyType>) -> usize {
    class.as_ptr().addr()
}

/// Whether a weak reference still points at `class`.
fn points_at(known: &Py<PyWeakrefReference>, class: &Bound<'_, PyType>) -> bool {
    (known.bind(class.py()).upgrade()).is_some_and(|known| known.is(class))
}

impl<P> Plans<P> {
    fn find(&self, class: &Bound<'_, PyType>) -> Option<Arc<P>> {
        let plans = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let kept = plans.get(&address(class))?;
        points_at(&kept.class, class).then(|| Arc::clone(&kept.plan))
    }

    fn insert(&self, class: &Bound<'_, PyType>, plan: Arc<P>) -> PyResult<Arc<P>> {
        let weak = PyWeakrefReference::new(class)?.unbind();
        // What is taken out under the lock is dropped after it. Dropping the last reference to
        // a class runs Python code, which may walk a tree of this grammar and want the lock.
        let mut alive = Vec::new();
        let mut plans = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        // Another thread may have made the same plan meanwhile, and then its plan is kept.
        if let Some(kept) = plans.get(&address(class))
            && points_at(&kept.class, class)
        {
            return Ok(Arc::clone(&kept.plan));
        }
        let gone = plans.extract_if(|_, kept| match kept.class.bind(class.py()).upgrade() {
            Some(known) => {
                alive.push(known);
                false
            }
            None => true,
        });
        let mut gone: Vec<Kept<P>> = gone.map(|(_, kept)| kept).collect();
        let kept = Kept {
            class: weak,
            plan: Arc::clone(&plan),
        };
        gone.extend(plans.insert(address(class), kept));
        drop(plans);
        drop((alive, gone));
        Ok(plan)
    }
}

/// The kinds of a grammar as the plan maker reads them: name, whether they are tokens, and the
/// fields a node of that name has, each with whether it can hold several items.
fn kind_table<'py>(py: Python<'py>, core: &GrammarCore) -> PyResult<Bound<'py, PyList>> {
    let mut rows = Vec::new();
    for (id, kind) in core.parser.kinds().iter().enumerate() {
        let fields = (core.name_fields[id].iter())
            .map(|&(field, count)| {
                let name = core.field_names[usize::from(field.0)].clone_ref(py);
                (name, count == FieldCount::Many)
            })
            .collect::<Vec<_>>();
        let row = (
            core.kind_names[id].clone_ref(py),
            kind.token,
            PyTuple::new(py, fields)?,
        );
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
    type Entry = (usize, String, u8, Vec<String>, u8);
    let made = make_plan.call1((class, kind_table(py, core)?))?;
    let (entries, default_node, default_token, positions): (
        Vec<Entry>,
        bool,
        bool,
        Option<StampEntry>,
    ) = made.extract()?;
    let positions = positions
        .map(|entry| Stamp::from_entry(py, entry))
        .transpose()?;
    let mut handlers: Vec<Option<Handler>> = (0..core.kind_names.len()).map(|_| None).collect();
    for (kind, name, mode, field_names, extras) in entries {
        let known = core.name_fields.get(kind).map_or(&[][..], Vec::as_slice);
        let mut fields = Vec::new();
        for field in &field_names {
            let id = core.field_ids.get(field).copied();
            let count = id.and_then(|id| known.iter().find(|&&(f, _)| f == id));
            match (id, count) {
                (Some(id), Some(&(_, count))) => fields.push((id, count)),
                _ => {
                    return Err(PyValueError::new_err(format!(
                        "{name} has no field {field}"
                    )));
                }
            }
        }
        if let Some(slot) = handlers.get_mut(kind) {
            *slot = Some(Handler {
                name: PyString::intern(py, &name).unbind(),
                mode: Mode::from_code(mode)?,
                fields,
                extras,
            });
        }
    }
    let plan = Arc::new(Plan {
        handlers,
        default_node,
        default_token,
        positions,
    });
    core.plans.insert(class, plan)
}

/// `Discard`: returned by a method, it removes the node or token from its parent.
#[pyclass(frozen, module = "fpeg", name = "DiscardType")]
pub struct DiscardType;

#[pymethods]
impl DiscardType {
    fn __repr__(&self) -> &'static str {
        "Discard"
    }
}

/// The one `Discard`, and the object that says a token has no builder.
static DISCARD: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
static NO_VALUE: PyOnceLock<Py<PyAny>> = PyOnceLock::new();

pub fn discard(py: Python<'_>) -> PyResult<&Bound<'_, PyAny>> {
    let discard =
        DISCARD.get_or_try_init(py, || Ok::<_, PyErr>(Py::new(py, DiscardType)?.into_any()))?;
    Ok(discard.bind(py))
}

pub fn no_value(py: Python<'_>) -> PyResult<&Bound<'_, PyAny>> {
    let value =
        NO_VALUE.get_or_try_init(py, || PyAny::type_object(py).call0().map(Bound::unbind))?;
    Ok(value.bind(py))
}

/// A value on the stack: the entry it came from, the field that entry fills in its parent, and
/// the object, or none for an entry that no method reached.
struct Value {
    entry: u32,
    field: FieldId,
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
    /// The children of the node at hand with their fields, and the arguments of the call at
    /// hand, kept so that their space is reused from node to node.
    children: Vec<(FieldId, Py<PyAny>)>,
    arguments: Vec<Py<PyAny>>,
    /// The entry whose method last yielded or discarded. A node above it cannot come back as its
    /// view, and at the end it says whether the values of the top are a list.
    spliced: Option<u32>,
    /// Positions of the tokens the scan makes, which come in input order.
    cursor: Option<LineCursor<'p>>,
    /// The classes whose instances get positions, from the class's record at this transform.
    position_types: Option<Bound<'py, PyTuple>>,
    /// The byte offsets of the characters beyond the BMP, which take two UTF-16 units, found
    /// at the first stamp in that unit.
    beyond_bmp: OnceCell<Vec<u32>>,
    discard: Bound<'py, PyAny>,
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

    /// The error of a call with a note saying which method and which item.
    fn noted(&self, error: PyErr, method: &str, node: NodeRef<'_>) -> PyErr {
        let position = self.parsed.position(node.span().0);
        let class = (self.transformer.get_type().qualname())
            .map_or_else(|_| "Transformer".to_owned(), |name| name.to_string());
        let what = if node.is_token() { "token" } else { "node" };
        let note = format!(
            "fpeg: in {class}.{method}, for the {what} {} at line {}, column {}",
            node.kind_name(),
            position.line,
            position.column
        );
        let _ = error.add_note(self.py, note);
        error
    }

    /// The error of a builder with a note saying which builder and which item.
    fn noted_builder(&self, error: PyErr, builder: &str, node: NodeRef<'_>) -> PyErr {
        let position = self.parsed.position(node.span().0);
        let what = if node.is_token() { "token" } else { "node" };
        let note = format!(
            "fpeg: in the builder {builder} of {}, for the {what} at line {}, column {}",
            node.kind_name(),
            position.line,
            position.column
        );
        let _ = error.add_note(self.py, note);
        error
    }

    /// Push what a method returned: nothing for `Discard`, each value of a generator, or the
    /// value, each with the position of the entry where the class asks for it.
    fn push(&mut self, entry: usize, field: FieldId, result: Bound<'py, PyAny>) -> PyResult<()> {
        if result.is(&self.discard) {
            self.spliced = Some(entry as u32);
            return Ok(());
        }
        // SAFETY: `result` is a live object.
        let generator = unsafe { ffi::PyGen_CheckExact(result.as_ptr()) } != 0;
        if generator {
            self.spliced = Some(entry as u32);
            for value in result.try_iter()? {
                let value = value?;
                // A yielded Discard puts nothing, as a returned one does.
                if !value.is(&self.discard) {
                    self.stamp(entry, &value)?;
                    self.stack.push(Value {
                        entry: entry as u32,
                        field,
                        object: Some(value.unbind()),
                    });
                }
            }
        } else {
            self.stamp(entry, &result)?;
            self.stack.push(Value {
                entry: entry as u32,
                field,
                object: Some(result.unbind()),
            });
        }
        Ok(())
    }

    /// Give a value the position of its entry, where the class asks for positions and the value
    /// is an instance of their classes without the first attribute yet. A value passed up from a
    /// child has its own already.
    fn stamp(&self, entry: usize, value: &Bound<'py, PyAny>) -> PyResult<()> {
        let (Some(stamp), Some(types)) = (&self.plan.positions, &self.position_types) else {
            return Ok(());
        };
        let py = self.py;
        if !value.is_instance(types)? || value.getattr_opt(stamp.names[0].bind(py))?.is_some() {
            return Ok(());
        }
        let Some(node) = self.parsed.tree.node(entry) else {
            return Ok(());
        };
        let (start, end) = node.span();
        let (start_line, start_column) = self.position(start, stamp);
        let (end_line, end_column) = self.position(end, stamp);
        for (name, number) in
            stamp
                .names
                .iter()
                .zip([start_line, start_column, end_line, end_column])
        {
            value.setattr(name.bind(py), number)?;
        }
        Ok(())
    }

    /// The line and column of a byte offset as a stamp counts them.
    fn position(&self, offset: u32, stamp: &Stamp) -> (i64, i64) {
        let parsed = self.parsed;
        let lines = parsed.lines();
        let offset = offset as usize;
        let start = lines.line_start(offset);
        let code_points =
            || lines.char_offset(&parsed.source, offset) - lines.char_offset(&parsed.source, start);
        let column = match stamp.unit {
            Unit::Utf8 => offset - start,
            Unit::CodePoints => code_points(),
            Unit::Utf16 => {
                let beyond = self.beyond_bmp.get_or_init(|| {
                    (parsed.source.char_indices())
                        .filter(|(_, c)| c.len_utf16() == 2)
                        .map(|(at, _)| at as u32)
                        .collect()
                });
                let before = |at: usize| beyond.partition_point(|&b| (b as usize) < at);
                code_points() + before(offset) - before(start)
            }
        };
        // A record keeps its bases to 32 bits, so no position reaches the bound.
        let line = (lines.line(offset) as i64).saturating_add(stamp.line_base);
        (line, (column as i64).saturating_add(stamp.column_base))
    }

    fn token(&mut self, node: NodeRef<'_>) -> PyResult<()> {
        let index = node.index();
        let kind = node.kind().0 as usize;
        let handler = (self.plan.handlers.get(kind)).and_then(Option::as_ref);
        if self.visit_tokens
            && let Some(handler) = handler
        {
            let method = self.method(kind, &handler.name)?;
            let result = match handler.mode {
                Mode::Nothing => vectorcall(&method, &[], &[]),
                _ => vectorcall(&method, &[self.make_token(node)?.unbind()], &[]),
            };
            let py = self.py;
            let name = || handler.name.bind(py).to_string();
            let result = result.map_err(|e| self.noted(e, &name(), node))?;
            // A method that yields runs while its values are taken.
            return (self.push(index, node.field(), result))
                .map_err(|e| self.noted(e, &name(), node));
        }
        let parsed = self.parsed;
        if let Some(builder) = parsed.grammar.builders.get(kind).and_then(Option::as_ref) {
            let (start, end) = node.span();
            let result = builders::token_value(self.py, builder, parsed.text(start, end))
                .map_err(|e| self.noted_builder(e, builder.name(), node))?;
            return (self.push(index, node.field(), result))
                .map_err(|e| self.noted_builder(e, builder.name(), node));
        }
        if self.visit_tokens && self.plan.default_token {
            let token = self.make_token(node)?;
            let result = (self.transformer)
                .call_method1(intern!(self.py, "__default_token__"), (token,))
                .map_err(|e| self.noted(e, "__default_token__", node))?;
            return (self.push(index, node.field(), result))
                .map_err(|e| self.noted(e, "__default_token__", node));
        }
        self.stack.push(Value {
            entry: index as u32,
            field: node.field(),
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
        let kind = node.kind().0 as usize;
        let handler = (self.plan.handlers.get(kind)).and_then(Option::as_ref);
        let builder = (self.parsed.grammar.builders.get(kind)).and_then(Option::as_ref);
        // Entries come in postorder, so a splice from `first` on removed or replaced a child.
        if handler.is_none()
            && builder.is_none()
            && !self.plan.default_node
            && self.spliced.is_none_or(|entry| (entry as usize) < first)
            && self.stack[base..]
                .iter()
                .all(|child| child.object.is_none())
        {
            self.stack.truncate(base);
            self.stack.push(Value {
                entry: index as u32,
                field: node.field(),
                object: None,
            });
            return Ok(());
        }
        let mut children = self.take_children(base)?;
        let result = self.value(node, kind, handler, builder, &children);
        children.clear();
        self.children = children;
        // A method or builder that yields runs while its values are taken.
        (self.push(index, node.field(), result?)).map_err(|e| match (handler, builder) {
            (Some(handler), _) => self.noted(e, &handler.name.bind(self.py).to_string(), node),
            (None, Some(builder)) => self.noted_builder(e, builder.name(), node),
            (None, None) => self.noted(e, "__default__", node),
        })
    }

    /// Take the values from `base` on off the stack as objects with their fields, into the space
    /// kept for children.
    fn take_children(&mut self, base: usize) -> PyResult<Vec<(FieldId, Py<PyAny>)>> {
        let (py, parsed) = (self.py, self.parsed);
        let mut children = std::mem::take(&mut self.children);
        for value in self.stack.drain(base..) {
            let object = match value.object {
                Some(object) => object,
                None => item(py, parsed, value.entry as usize)?,
            };
            children.push((value.field, object));
        }
        Ok(children)
    }

    /// The value of a node from its method, its builder or `__default__`, else a tree of the
    /// values of its children.
    fn value(
        &mut self,
        node: NodeRef<'_>,
        kind: usize,
        handler: Option<&Handler>,
        builder: Option<&builders::Builder>,
        children: &[(FieldId, Py<PyAny>)],
    ) -> PyResult<Bound<'py, PyAny>> {
        let (py, parsed) = (self.py, self.parsed);
        if let Some(handler) = handler {
            return self.call(node, kind, handler, children);
        }
        if let Some(builder) = builder {
            let (start, end) = node.span();
            let names = &parsed.grammar.field_names;
            return builders::node_value(py, builder, parsed.text(start, end), children, names)
                .map_err(|e| self.noted_builder(e, builder.name(), node));
        }
        let data = parsed.grammar.kind_names[kind].clone_ref(py);
        let list = PyList::new(py, children.iter().map(|(_, object)| object.clone_ref(py)))?;
        if self.plan.default_node {
            let meta = self.meta(node)?;
            return (self.transformer)
                .call_method1(intern!(py, "__default__"), (data, list, meta))
                .map_err(|e| self.noted(e, "__default__", node));
        }
        let origin = Some((Arc::clone(parsed), node.index() as u32));
        let tree = Tree::owned(data.into_any(), list.unbind(), origin);
        Ok(Bound::new(py, tree)?.into_any())
    }

    /// Call the method of a node as its plan says.
    fn call(
        &mut self,
        node: NodeRef<'_>,
        kind: usize,
        handler: &Handler,
        children: &[(FieldId, Py<PyAny>)],
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = self.py;
        let method = self.method(kind, &handler.name)?;
        let objects = || children.iter().map(|(_, object)| object.clone_ref(py));
        let list = || PyList::new(py, objects()).map(|list| list.into_any().unbind());
        let mut args = std::mem::take(&mut self.arguments);
        match handler.mode {
            Mode::List => args.push(list()?),
            Mode::Inline => args.extend(objects()),
            Mode::Fields => {
                for &(field, count) in &handler.fields {
                    let mut items = (children.iter())
                        .filter(|(f, _)| *f == field)
                        .map(|(_, object)| object.clone_ref(py));
                    args.push(match count {
                        FieldCount::Many => {
                            let list = PyList::empty(py);
                            for item in items {
                                list.append(item)?;
                            }
                            list.into_any().unbind()
                        }
                        FieldCount::One | FieldCount::Optional => {
                            let first = items.next();
                            let more = items.count();
                            if more > 0 {
                                let name = &self.parsed.grammar.field_names[usize::from(field.0)];
                                let message = format!(
                                    "the field {} holds one item, and its children gave {} values",
                                    name.bind(py),
                                    more + 1
                                );
                                let error = PyTypeError::new_err(message);
                                return Err(self.noted(
                                    error,
                                    &handler.name.bind(py).to_string(),
                                    node,
                                ));
                            }
                            first.unwrap_or_else(|| py.None())
                        }
                    });
                }
            }
            Mode::Meta => {
                args.push(self.meta(node)?);
                args.push(list()?);
            }
            Mode::MetaInline => {
                args.push(self.meta(node)?);
                args.extend(objects());
            }
            Mode::Tree => {
                let data = self.parsed.grammar.kind_names[kind].clone_ref(py);
                let origin = Some((Arc::clone(self.parsed), node.index() as u32));
                let children = PyList::new(py, objects())?.unbind();
                let tree = Tree::owned(data.into_any(), children, origin);
                args.push(Py::new(py, tree)?.into_any());
            }
            Mode::Nothing | Mode::Token => {}
        }
        let mut keywords: Vec<(&'static str, Py<PyAny>)> = Vec::new();
        if handler.extras & EXTRA_META != 0 {
            keywords.push(("meta", self.meta(node)?));
        }
        if handler.extras & EXTRA_NODE != 0 {
            keywords.push(("node", item(py, self.parsed, node.index())?));
        }
        if handler.extras & EXTRA_CHILDREN != 0 {
            keywords.push(("children", list()?));
        }
        let result = vectorcall(&method, &args, &keywords)
            .map_err(|error| self.noted(error, &handler.name.bind(py).to_string(), node));
        args.clear();
        self.arguments = args;
        result
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
    let count = 1 + args.len() + keywords.len();
    let mut on_stack = [null_mut(); CALL_STACK_POINTERS];
    let mut on_heap = Vec::new();
    let pointers: &mut [*mut ffi::PyObject] = if count <= CALL_STACK_POINTERS {
        &mut on_stack[..count]
    } else {
        on_heap.resize(count, null_mut());
        &mut on_heap
    };
    let values =
        (args.iter().map(Py::as_ptr)).chain(keywords.iter().map(|(_, value)| value.as_ptr()));
    for (slot, value) in pointers[1..].iter_mut().zip(values) {
        *slot = value;
    }
    let names = if keywords.is_empty() {
        None
    } else {
        let names = keywords.iter().map(|(name, _)| PyString::intern(py, name));
        Some(PyTuple::new(py, names)?)
    };
    let names_pointer = names.as_ref().map_or(null_mut(), |n| n.as_ptr());
    // SAFETY: the pointers borrow from `args` and `keywords`, which outlive the call. The count
    // leaves out the free slot and the keyword values, whose names `names` holds. The pointer
    // comes from a mutable borrow, since the callee may write the free slot for the call.
    let result = unsafe {
        ffi::PyObject_Vectorcall(
            callable.as_ptr(),
            pointers.as_mut_ptr().add(1),
            args.len() | ffi::PY_VECTORCALL_ARGUMENTS_OFFSET,
            names_pointer,
        )
    };
    // SAFETY: vectorcall returns a new reference, or null with an exception set.
    unsafe { Bound::from_owned_ptr_or_err(py, result) }
}

/// Whether a tree is a view of a parse.
#[pyfunction]
pub fn is_view(tree: &Bound<'_, Tree>) -> bool {
    tree.get().parse_entry().is_some()
}

/// Transform a view with the plan of the transformer's class, and return the values its node
/// gives its parent: one in a tuple, or a list of what its method yielded or discarded.
#[pyfunction]
pub fn scan<'py>(
    py: Python<'py>,
    transformer: &Bound<'py, PyAny>,
    view: &Bound<'py, Tree>,
    make_plan: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
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
    let position_types = match plan.positions {
        Some(_) => Some(
            (transformer.get_type())
                .getattr(intern!(py, "__fpeg_positions__"))?
                .getattr(intern!(py, "types"))?
                .cast_into::<PyTuple>()?,
        ),
        None => None,
    };
    let mut scan = Scan {
        py,
        transformer,
        parsed,
        plan: &plan,
        visit_tokens,
        methods: (0..parsed.grammar.kind_names.len()).map(|_| None).collect(),
        stack: Vec::new(),
        children: Vec::new(),
        arguments: Vec::new(),
        spliced: None,
        cursor: None,
        position_types,
        beyond_bmp: OnceCell::new(),
        discard: discard(py)?.clone(),
    };
    scan.run(index as usize)?;
    let values = std::mem::take(&mut scan.stack);
    // A node that nothing changed gives back the view it came as.
    if let [value] = values.as_slice()
        && value.object.is_none()
        && value.entry == index
    {
        return Ok(PyTuple::new(py, [view.clone().into_any()])?.into_any());
    }
    let objects = scan.objects(values)?;
    // At the top, where no parent takes them, what a method yields or discards is a list.
    if scan.spliced == Some(index) {
        return Ok(PyList::new(py, objects)?.into_any());
    }
    Ok(PyTuple::new(py, objects)?.into_any())
}

/// The plan of an Interpreter or Visitor class for the grammar of a view: what `make_plan`
/// gave for the class and the kinds of the grammar when the two first met. The walk is in Python, so the
/// plan is whatever its plan maker returns. `None` for a tree built in Python, which has no
/// grammar.
#[pyfunction]
pub fn walk_plan<'py>(
    py: Python<'py>,
    class: &Bound<'py, PyType>,
    view: &Bound<'py, Tree>,
    make_plan: &Bound<'py, PyAny>,
) -> PyResult<Option<Py<PyAny>>> {
    let Some((parsed, _)) = view.get().parse_entry() else {
        return Ok(None);
    };
    let core = &parsed.grammar;
    if let Some(plan) = core.walk_plans.find(class) {
        return Ok(Some(plan.clone_ref(py)));
    }
    let made = make_plan.call1((class, kind_table(py, core)?))?.unbind();
    let plan = core.walk_plans.insert(class, Arc::new(made))?;
    Ok(Some(plan.clone_ref(py)))
}

/// The value the builder of a token's terminal gives it, or `NO_VALUE` when it has none: a
/// token made in Python, or by a grammar that is gone, has no builder.
#[pyfunction]
pub fn token_value<'py>(py: Python<'py>, token: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let grammar = token::origin(token).and_then(|(serial, kind)| Some((grammar_of(serial)?, kind)));
    let Some((grammar, kind)) = grammar else {
        return Ok(no_value(py)?.clone());
    };
    let Some(builder) = grammar.builders.get(kind as usize).and_then(Option::as_ref) else {
        return Ok(no_value(py)?.clone());
    };
    let text = token.cast::<PyString>()?.to_cow()?;
    builders::token_value(py, builder, &text)
}
