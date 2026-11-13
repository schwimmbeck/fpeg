// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Tree` and `Meta`: views of the nodes of a parse, and trees built in Python.
//!
//! A view holds the parse and an entry index, so making one costs about as much as a string, and
//! its children are made when asked for and then kept. A tree built in Python, as Lark builds
//! them, holds its data and its list of children. Every walk over trees uses an explicit stack,
//! since left recursion makes trees as deep as their input is long, and refuses a tree built in
//! Python that contains itself instead of walking it forever.

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use fpeg::{FieldCount, LineIndex, Position};
use pyo3::PyTypeInfo;
use pyo3::exceptions::{PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedStr;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyIterator, PyList, PyTuple, PyType};

use crate::grammar::GrammarCore;
use crate::token::{self, Positions};

/// A parse: its tree, the text it read and the grammar that read it.
pub struct Parsed {
    pub tree: fpeg::Tree,
    pub source: PyBackedStr,
    pub grammar: Arc<GrammarCore>,
    lines: OnceLock<LineIndex>,
}

impl Parsed {
    pub fn new(tree: fpeg::Tree, source: PyBackedStr, grammar: Arc<GrammarCore>) -> Parsed {
        Parsed {
            tree,
            source,
            grammar,
            lines: OnceLock::new(),
        }
    }

    /// The lines of the text, indexed on first need.
    pub fn lines(&self) -> &LineIndex {
        self.lines.get_or_init(|| LineIndex::new(&self.source))
    }

    pub fn position(&self, offset: u32) -> Position {
        self.lines().position(&self.source, offset as usize)
    }

    /// The text of a byte span.
    pub fn text(&self, start: u32, end: u32) -> &str {
        self.source.get(start as usize..end as usize).unwrap_or("")
    }
}

/// The positions of a token from where it starts and ends.
pub fn token_positions(start: Position, end: Position) -> Positions {
    let signed = |value: usize| isize::try_from(value).unwrap_or(isize::MAX);
    Positions {
        start_pos: signed(start.offset),
        end_pos: signed(end.offset),
        line: signed(start.line),
        column: signed(start.column),
        end_line: signed(end.line),
        end_column: signed(end.column),
    }
}

/// The Python object for an entry of a parse: a `Token` for a token, a view for a node.
pub fn item(py: Python<'_>, parsed: &Arc<Parsed>, index: usize) -> PyResult<Py<PyAny>> {
    let node = (parsed.tree.node(index))
        .ok_or_else(|| PyValueError::new_err("a bug in fpeg: an entry past the end of a tree"))?;
    if node.is_token() {
        let (start, end) = node.span();
        let positions = token_positions(parsed.position(start), parsed.position(end));
        let kind = node.kind().0;
        let name = parsed.grammar.kind_names[kind as usize].bind(py);
        let serial = parsed.grammar.serial;
        let token = token::make(py, parsed.text(start, end), name, positions, serial, kind)?;
        Ok(token.unbind())
    } else {
        Ok(Py::new(py, Tree::view(Arc::clone(parsed), index as u32))?.into_any())
    }
}

/// Where a node of a parse spans, as `Tree.meta` gives it.
#[pyclass(frozen, module = "fpeg", get_all)]
pub struct Meta {
    /// The line and column where the node starts, from 1, the column in code points.
    line: Option<usize>,
    column: Option<usize>,
    /// The line and column where it ends, the column after its last character.
    end_line: Option<usize>,
    end_column: Option<usize>,
    /// The code points before its start and before its end.
    start_pos: Option<usize>,
    end_pos: Option<usize>,
    /// Whether the node holds no text.
    empty: bool,
}

impl Meta {
    /// Where a byte span of a parse lies.
    pub fn of(parsed: &Parsed, start: u32, end: u32) -> Meta {
        let (first, last) = (parsed.position(start), parsed.position(end));
        Meta {
            line: Some(first.line),
            column: Some(first.column),
            end_line: Some(last.line),
            end_column: Some(last.column),
            start_pos: Some(first.offset),
            end_pos: Some(last.offset),
            empty: start == end,
        }
    }
}

#[pymethods]
impl Meta {
    #[new]
    #[pyo3(signature = (line = None, column = None, end_line = None, end_column = None, start_pos = None, end_pos = None, empty = true))]
    fn new(
        line: Option<usize>,
        column: Option<usize>,
        end_line: Option<usize>,
        end_column: Option<usize>,
        start_pos: Option<usize>,
        end_pos: Option<usize>,
        empty: bool,
    ) -> Meta {
        Meta {
            line,
            column,
            end_line,
            end_column,
            start_pos,
            end_pos,
            empty,
        }
    }

    fn __repr__(&self) -> String {
        let shown =
            |value: Option<usize>| value.map_or_else(|| "None".to_owned(), |v| v.to_string());
        format!(
            "Meta(line={}, column={}, end_line={}, end_column={}, start_pos={}, end_pos={}, empty={})",
            shown(self.line),
            shown(self.column),
            shown(self.end_line),
            shown(self.end_column),
            shown(self.start_pos),
            shown(self.end_pos),
            if self.empty { "True" } else { "False" },
        )
    }

    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        let m = slf.get();
        let args = (
            m.line,
            m.column,
            m.end_line,
            m.end_column,
            m.start_pos,
            m.end_pos,
            m.empty,
        );
        PyTuple::new(
            slf.py(),
            [
                slf.get_type().into_any(),
                args.into_pyobject(slf.py())?.into_any(),
            ],
        )
    }
}

enum Inner {
    /// A node of a parse, by its entry.
    View { parsed: Arc<Parsed>, index: u32 },
    /// A tree built in Python, or by a transform for a node whose children changed, which keeps
    /// the node it came from for its positions.
    Owned {
        data: Py<PyAny>,
        origin: Option<(Arc<Parsed>, u32)>,
    },
}

/// The type of the children of a view, `fpeg._tree.Children`, found on first need.
static CHILDREN_TYPE: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// A node of a parse tree, as Lark's `Tree`: `data`, `children` and `meta`.
///
/// A tree from a parse is a view of the native tree, and its children are made when asked for.
/// `Tree(data, children, meta=None)` builds one in Python.
#[pyclass(frozen, module = "fpeg", subclass)]
pub struct Tree {
    inner: Inner,
    children: OnceLock<Py<PyList>>,
    meta: OnceLock<Py<PyAny>>,
}

impl Tree {
    pub fn view(parsed: Arc<Parsed>, index: u32) -> Tree {
        Tree {
            inner: Inner::View { parsed, index },
            children: OnceLock::new(),
            meta: OnceLock::new(),
        }
    }

    /// A tree with the data and children of a node of a parse, made when its children changed.
    pub fn owned(
        data: Py<PyAny>,
        children: Py<PyList>,
        origin: Option<(Arc<Parsed>, u32)>,
    ) -> Tree {
        Tree {
            inner: Inner::Owned { data, origin },
            children: OnceLock::from(children),
            meta: OnceLock::new(),
        }
    }

    pub fn data_object(&self, py: Python<'_>) -> Py<PyAny> {
        match &self.inner {
            Inner::View { parsed, index } => {
                let kind = parsed.tree.node(*index as usize).map_or(0, |n| n.kind().0);
                parsed.grammar.kind_names[kind as usize]
                    .clone_ref(py)
                    .into_any()
            }
            Inner::Owned { data, .. } => data.clone_ref(py),
        }
    }

    /// The children, made on first need for a view.
    pub fn children_list<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        if let Some(children) = self.children.get() {
            return Ok(children.bind(py).clone());
        }
        let Inner::View { parsed, index } = &self.inner else {
            return Ok(PyList::empty(py));
        };
        let node = (parsed.tree.node(*index as usize))
            .ok_or_else(|| PyValueError::new_err("a bug in fpeg: a view past the end of a tree"))?;
        // A list that refuses changes, filled through the C API, which its methods do not see.
        let children = (CHILDREN_TYPE.import(py, "fpeg._tree", "Children")?)
            .call0()?
            .cast_into::<PyList>()?;
        for child in node
            .children()
            .into_iter()
            .filter(|child| !child.is_hidden())
        {
            children.append(item(py, parsed, child.index())?)?;
        }
        // Another thread may have made them first, and then its list is the one kept.
        let _ = self.children.set(children.unbind());
        Ok(self
            .children
            .get()
            .map_or_else(|| PyList::empty(py), |c| c.bind(py).clone()))
    }

    /// The field names of the visible children of a view, in order, when the list of children
    /// still has one entry for each.
    fn child_fields(&self, count: usize) -> Vec<Option<String>> {
        let Inner::View { parsed, index } = &self.inner else {
            return vec![None; count];
        };
        let Some(node) = parsed.tree.node(*index as usize) else {
            return vec![None; count];
        };
        let fields: Vec<Option<String>> = (node.children().into_iter())
            .filter(|child| !child.is_hidden())
            .map(|child| child.field_name().map(str::to_owned))
            .collect();
        if fields.len() == count {
            fields
        } else {
            vec![None; count]
        }
    }

    /// The parse and entry of a view, `None` for a tree built in Python.
    pub fn parse_entry(&self) -> Option<(&Arc<Parsed>, u32)> {
        match &self.inner {
            Inner::View { parsed, index } => Some((parsed, *index)),
            Inner::Owned { .. } => None,
        }
    }

    fn origin(&self) -> Option<(&Arc<Parsed>, u32)> {
        match &self.inner {
            Inner::View { parsed, index } => Some((parsed, *index)),
            Inner::Owned { origin, .. } => origin.as_ref().map(|(parsed, index)| (parsed, *index)),
        }
    }
}

/// Whether two objects are views of one node, which makes them equal without a walk.
fn same_node(a: &Bound<'_, Tree>, b: &Bound<'_, Tree>) -> bool {
    match (&a.get().inner, &b.get().inner) {
        (
            Inner::View {
                parsed: p,
                index: i,
            },
            Inner::View {
                parsed: q,
                index: j,
            },
        ) => Arc::ptr_eq(p, q) && i == j,
        _ => false,
    }
}

/// The error for a tree built in Python that holds itself.
fn cycle() -> PyErr {
    PyValueError::new_err("the tree contains itself")
}

/// The steps of a walk that writes text: an item to write at a level, or text to write.
enum Step<'py> {
    Item(Bound<'py, PyAny>, usize, Option<String>),
    Text(&'static str),
    Leave(usize),
}

#[pymethods]
impl Tree {
    #[new]
    #[pyo3(signature = (data, children, meta = None))]
    fn new(
        py: Python<'_>,
        data: Py<PyAny>,
        children: Bound<'_, PyAny>,
        meta: Option<Py<PyAny>>,
    ) -> PyResult<Tree> {
        let children = match children.cast_into::<PyList>() {
            Ok(list) => list,
            Err(error) => PyList::type_object(py)
                .call1((error.into_inner(),))?
                .cast_into::<PyList>()?,
        };
        let tree = Tree::owned(data, children.unbind(), None);
        if let Some(meta) = meta {
            let _ = tree.meta.set(meta);
        }
        Ok(tree)
    }

    /// The node's kind: the name of its rule or alias.
    #[getter]
    fn data(&self, py: Python<'_>) -> Py<PyAny> {
        self.data_object(py)
    }

    /// The visible children: trees for nodes, tokens, or values a transform made.
    #[getter]
    fn children<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        self.children_list(py)
    }

    /// Where the node spans: Lark's line, column, end_line, end_column, start_pos and end_pos.
    #[getter]
    fn meta(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(meta) = self.meta.get() {
            return Ok(meta.clone_ref(py));
        }
        let meta = match self.origin() {
            Some((parsed, index)) => {
                let (start, end) = parsed
                    .tree
                    .node(index as usize)
                    .map_or((0, 0), |n| n.span());
                Meta::of(parsed, start, end)
            }
            None => Meta::new(None, None, None, None, None, None, true),
        };
        let _ = self.meta.set(Py::new(py, meta)?.into_any());
        Ok(self
            .meta
            .get()
            .map_or_else(|| py.None(), |m| m.clone_ref(py)))
    }

    /// The text of the node in its input, or `None` for a tree built in Python.
    #[getter]
    fn text(&self) -> Option<String> {
        let (parsed, index) = self.origin()?;
        let (start, end) = parsed.tree.node(index as usize)?.span();
        Some(parsed.text(start, end).to_owned())
    }

    /// The field `name` of the node: its item or `None` for a field of at most one item, a list
    /// for a field that can hold several.
    fn field(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        let Inner::View { parsed, index } = &self.inner else {
            return Err(PyTypeError::new_err(
                "fields belong to trees of a parse, and this tree was built in Python",
            ));
        };
        let node = (parsed.tree.node(*index as usize))
            .ok_or_else(|| PyValueError::new_err("a bug in fpeg: a view past the end of a tree"))?;
        let grammar = &parsed.grammar;
        let fields = &grammar.name_fields[node.kind().0 as usize];
        let field = grammar.field_ids.get(name).copied();
        let count = field
            .and_then(|f| fields.iter().find(|&&(id, _)| id == f))
            .map(|&(_, c)| c);
        let (Some(field), Some(count)) = (field, count) else {
            let names: Vec<String> = (fields.iter())
                .map(|&(id, _)| grammar.field_names[usize::from(id.0)].bind(py).to_string())
                .collect();
            let known = if names.is_empty() {
                "it has no fields".to_owned()
            } else {
                format!("its fields are {}", names.join(", "))
            };
            let message = format!("{} has no field {name}, {known}", node.kind_name());
            return Err(PyKeyError::new_err(message));
        };
        let mut items = Vec::new();
        for child in node.children() {
            if !child.is_hidden() && child.field() == field {
                items.push(item(py, parsed, child.index())?);
            }
        }
        match count {
            FieldCount::One | FieldCount::Optional => {
                Ok(items.into_iter().next().unwrap_or_else(|| py.None()))
            }
            FieldCount::Many => Ok(PyList::new(py, items)?.into_any().unbind()),
        }
    }

    /// The tree as indented lines, as Lark writes it, with `name=` before an item that fills a
    /// field of a node of a parse.
    #[pyo3(signature = (indent_str = "  "))]
    fn pretty(slf: &Bound<'_, Self>, indent_str: &str) -> PyResult<String> {
        let py = slf.py();
        let mut out = String::new();
        let mut path: HashSet<usize> = HashSet::new();
        let mut pending = vec![Step::Item(slf.clone().into_any(), 0, None)];
        while let Some(step) = pending.pop() {
            let (obj, level, field) = match step {
                Step::Item(obj, level, field) => (obj, level, field),
                Step::Leave(key) => {
                    path.remove(&key);
                    continue;
                }
                Step::Text(text) => {
                    out.push_str(text);
                    continue;
                }
            };
            out.push_str(&indent_str.repeat(level));
            if let Some(field) = field {
                out.push_str(&field);
                out.push('=');
            }
            let Ok(tree) = obj.cast::<Tree>() else {
                out.push_str(&obj.str()?.to_string());
                out.push('\n');
                continue;
            };
            let key = obj.as_ptr().addr();
            if !path.insert(key) {
                return Err(cycle());
            }
            pending.push(Step::Leave(key));
            out.push_str(&tree.get().data_object(py).bind(py).str()?.to_string());
            let children = tree.get().children_list(py)?;
            let fields = tree.get().child_fields(children.len());
            if children.len() == 1 && !children.get_item(0)?.is_instance_of::<Tree>() {
                out.push('\t');
                if let Some(field) = &fields[0] {
                    out.push_str(field);
                    out.push('=');
                }
                out.push_str(&children.get_item(0)?.str()?.to_string());
                out.push('\n');
                continue;
            }
            out.push('\n');
            for (child, field) in children.iter().zip(fields).rev() {
                pending.push(Step::Item(child, level + 1, field));
            }
        }
        Ok(out)
    }

    /// Every subtree, children before their parents, each once.
    fn iter_subtrees<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyIterator>> {
        let py = slf.py();
        let mut seen: HashSet<usize> = HashSet::new();
        let mut out = Vec::new();
        let mut pending = vec![(slf.clone().into_any(), false)];
        while let Some((obj, done)) = pending.pop() {
            if done {
                out.push(obj);
                continue;
            }
            let Ok(tree) = obj.cast::<Tree>() else {
                continue;
            };
            if !seen.insert(obj.as_ptr().addr()) {
                continue;
            }
            let children = tree.get().children_list(py)?;
            pending.push((obj.clone(), true));
            for child in children.iter().rev() {
                pending.push((child, false));
            }
        }
        PyList::new(py, out)?.try_iter()
    }

    /// Every subtree, parents before their children.
    fn iter_subtrees_topdown<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyIterator>> {
        let py = slf.py();
        let mut seen: HashSet<usize> = HashSet::new();
        let mut out = Vec::new();
        let mut pending = vec![slf.clone().into_any()];
        while let Some(obj) = pending.pop() {
            let Ok(tree) = obj.cast::<Tree>() else {
                continue;
            };
            if !seen.insert(obj.as_ptr().addr()) {
                continue;
            }
            let children = tree.get().children_list(py)?;
            out.push(obj.clone());
            for child in children.iter().rev() {
                pending.push(child);
            }
        }
        PyList::new(py, out)?.try_iter()
    }

    /// The subtrees for which `pred` is true, children before their parents.
    fn find_pred<'py>(
        slf: &Bound<'py, Self>,
        pred: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyIterator>> {
        let mut out = Vec::new();
        for subtree in Tree::iter_subtrees(slf)? {
            let subtree = subtree?;
            if pred.call1((&subtree,))?.is_truthy()? {
                out.push(subtree);
            }
        }
        PyList::new(slf.py(), out)?.try_iter()
    }

    /// The subtrees whose data equals `data`, children before their parents.
    fn find_data<'py>(
        slf: &Bound<'py, Self>,
        data: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyIterator>> {
        let py = slf.py();
        let mut out = Vec::new();
        for subtree in Tree::iter_subtrees(slf)? {
            let subtree = subtree?;
            let tree = subtree.cast::<Tree>()?;
            if tree.get().data_object(py).bind(py).eq(data)? {
                out.push(subtree);
            }
        }
        PyList::new(py, out)?.try_iter()
    }

    /// The values below the tree that are no trees and for which `pred` is true, in order.
    fn scan_values<'py>(
        slf: &Bound<'py, Self>,
        pred: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyIterator>> {
        let py = slf.py();
        let mut out = Vec::new();
        let mut path: HashSet<usize> = HashSet::new();
        let mut pending = vec![Step::Item(slf.clone().into_any(), 0, None)];
        while let Some(step) = pending.pop() {
            let obj = match step {
                Step::Item(obj, ..) => obj,
                Step::Leave(key) => {
                    path.remove(&key);
                    continue;
                }
                Step::Text(_) => continue,
            };
            let Ok(tree) = obj.cast::<Tree>() else {
                if pred.call1((&obj,))?.is_truthy()? {
                    out.push(obj);
                }
                continue;
            };
            let key = obj.as_ptr().addr();
            if !path.insert(key) {
                return Err(cycle());
            }
            pending.push(Step::Leave(key));
            for child in tree.get().children_list(py)?.iter().rev() {
                pending.push(Step::Item(child, 0, None));
            }
        }
        PyList::new(py, out)?.try_iter()
    }

    /// A tree with the same data and meta and a new list of the same children.
    fn copy(&self, py: Python<'_>) -> PyResult<Tree> {
        self.__copy__(py)
    }

    fn __copy__(&self, py: Python<'_>) -> PyResult<Tree> {
        let children = PyList::new(py, self.children_list(py)?.iter())?;
        let tree = Tree::owned(self.data_object(py), children.unbind(), None);
        let _ = tree.meta.set(self.meta(py)?);
        Ok(tree)
    }

    fn __richcmp__(
        slf: &Bound<'_, Self>,
        other: &Bound<'_, PyAny>,
        op: pyo3::basic::CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let equal = || trees_equal(slf.as_any(), other);
        match op {
            pyo3::basic::CompareOp::Eq => {
                Ok(equal()?.into_pyobject(py)?.to_owned().into_any().unbind())
            }
            pyo3::basic::CompareOp::Ne => Ok((!equal()?)
                .into_pyobject(py)?
                .to_owned()
                .into_any()
                .unbind()),
            _ => Ok(py.NotImplemented()),
        }
    }

    fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
        tree_hash(slf.as_any())
    }

    fn __repr__(slf: &Bound<'_, Self>) -> PyResult<String> {
        let py = slf.py();
        let mut out = String::new();
        let mut path: HashSet<usize> = HashSet::new();
        let mut pending = vec![Step::Item(slf.clone().into_any(), 0, None)];
        while let Some(step) = pending.pop() {
            let obj = match step {
                Step::Item(obj, ..) => obj,
                Step::Text(text) => {
                    out.push_str(text);
                    continue;
                }
                Step::Leave(key) => {
                    path.remove(&key);
                    out.push_str("])");
                    continue;
                }
            };
            let Ok(tree) = obj.cast::<Tree>() else {
                out.push_str(&obj.repr()?.to_string());
                continue;
            };
            let key = obj.as_ptr().addr();
            if !path.insert(key) {
                return Err(cycle());
            }
            out.push_str("Tree(");
            out.push_str(&tree.get().data_object(py).bind(py).repr()?.to_string());
            out.push_str(", [");
            pending.push(Step::Leave(key));
            let children: Vec<Bound<'_, PyAny>> = tree.get().children_list(py)?.iter().collect();
            for (i, child) in children.into_iter().enumerate().rev() {
                pending.push(Step::Item(child, 0, None));
                if i > 0 {
                    pending.push(Step::Text(", "));
                }
            }
        }
        Ok(out)
    }

    /// Pickle the tree flat, as its items in postorder, so that a deep tree pickles without deep
    /// recursion.
    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        let py = slf.py();
        let shape = PyList::empty(py);
        let values = PyList::empty(py);
        let mut path: HashSet<usize> = HashSet::new();
        let mut pending = vec![(slf.clone().into_any(), false)];
        while let Some((obj, done)) = pending.pop() {
            let Ok(tree) = obj.cast::<Tree>() else {
                shape.append(-1)?;
                values.append(obj)?;
                continue;
            };
            let key = obj.as_ptr().addr();
            let children = tree.get().children_list(py)?;
            if done {
                path.remove(&key);
                shape.append(children.len())?;
                values.append((tree.get().data_object(py), tree.get().meta(py)?))?;
                continue;
            }
            if !path.insert(key) {
                return Err(cycle());
            }
            pending.push((obj.clone(), true));
            for child in children.iter().rev() {
                pending.push((child, false));
            }
        }
        let rebuild = py.import("fpeg._fpeg")?.getattr("rebuild_tree")?;
        PyTuple::new(py, [rebuild, PyTuple::new(py, [shape, values])?.into_any()])
    }
}

/// Rebuild a tree pickled by `Tree.__reduce__`: items in postorder, -1 for a value and the
/// number of children for a tree.
#[pyfunction]
pub fn rebuild_tree(
    py: Python<'_>,
    shape: Vec<isize>,
    values: Bound<'_, PyList>,
) -> PyResult<Py<PyAny>> {
    let mut stack: Vec<Py<PyAny>> = Vec::new();
    for (count, value) in shape.into_iter().zip(values.iter()) {
        if count < 0 {
            stack.push(value.unbind());
            continue;
        }
        let count = count as usize;
        if count > stack.len() {
            return Err(PyValueError::new_err("a damaged pickle of a tree"));
        }
        let children = PyList::new(py, stack.split_off(stack.len() - count))?;
        let (data, meta): (Py<PyAny>, Py<PyAny>) = value.extract()?;
        let tree = Tree::owned(data, children.unbind(), None);
        let _ = tree.meta.set(meta);
        stack.push(Py::new(py, tree)?.into_any());
    }
    match stack.pop() {
        Some(tree) if stack.is_empty() => Ok(tree),
        _ => Err(PyValueError::new_err("a damaged pickle of a tree")),
    }
}

/// Lark's equality: two trees are equal when their data and children are, a tree never equals
/// anything else.
fn trees_equal(a: &Bound<'_, PyAny>, b: &Bound<'_, PyAny>) -> PyResult<bool> {
    let py = a.py();
    let mut path: HashSet<(usize, usize)> = HashSet::new();
    // Each pair to compare, or the end of a pair whose children are compared.
    let mut pending: Vec<Option<(Bound<'_, PyAny>, Bound<'_, PyAny>)>> =
        vec![Some((a.clone(), b.clone()))];
    let mut leaving: Vec<(usize, usize)> = Vec::new();
    while let Some(next) = pending.pop() {
        let Some((x, y)) = next else {
            if let Some(key) = leaving.pop() {
                path.remove(&key);
            }
            continue;
        };
        // An object equals itself, as in Python's lists, also a NaN.
        if x.is(&y) {
            continue;
        }
        match (x.cast::<Tree>(), y.cast::<Tree>()) {
            (Ok(s), Ok(t)) => {
                if same_node(s, t) {
                    continue;
                }
                let key = (x.as_ptr().addr(), y.as_ptr().addr());
                if !path.insert(key) {
                    return Err(cycle());
                }
                let (s, t) = (s.get(), t.get());
                if !s.data_object(py).bind(py).eq(t.data_object(py).bind(py))? {
                    return Ok(false);
                }
                let (left, right) = (s.children_list(py)?, t.children_list(py)?);
                if left.len() != right.len() {
                    return Ok(false);
                }
                pending.push(None);
                leaving.push(key);
                for (l, r) in left.iter().zip(right.iter()).rev() {
                    pending.push(Some((l, r)));
                }
            }
            (Err(_), Err(_)) => {
                if !x.eq(&y)? {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}

/// A hash of a tree's data and children that equal trees share.
fn tree_hash(root: &Bound<'_, PyAny>) -> PyResult<isize> {
    let py = root.py();
    // The hashes of finished children, and for each tree being walked the stack height where
    // its children's hashes start.
    let mut hashes: Vec<isize> = Vec::new();
    let mut path: HashSet<usize> = HashSet::new();
    let mut pending: Vec<(Bound<'_, PyAny>, Option<usize>)> = vec![(root.clone(), None)];
    while let Some((obj, start)) = pending.pop() {
        let Ok(tree) = obj.cast::<Tree>() else {
            hashes.push(obj.hash()?);
            continue;
        };
        let key = obj.as_ptr().addr();
        if let Some(start) = start {
            path.remove(&key);
            let mut hash = tree.get().data_object(py).bind(py).hash()? as u64;
            for child in hashes.drain(start..) {
                // The combining step of CPython's tuple hash, order sensitive.
                hash = (hash ^ child as u64)
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .rotate_left(31);
            }
            hashes.push(hash as isize);
            continue;
        }
        if !path.insert(key) {
            return Err(cycle());
        }
        pending.push((obj.clone(), Some(hashes.len())));
        for child in tree.get().children_list(py)?.iter().rev() {
            pending.push((child, None));
        }
    }
    let hash = hashes.pop().unwrap_or(0);
    // -1 means an error to CPython.
    Ok(if hash == -1 { -2 } else { hash })
}
