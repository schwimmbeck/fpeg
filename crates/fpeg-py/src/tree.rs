// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Tree` and `Meta`: views of the nodes of a parse, and trees built in Python.
//!
//! A view holds the parse and an entry index, so making one costs about as much as a string, and
//! its children are made when asked for and then kept. A tree built in Python, as Lark builds
//! them, holds its data and its list of children.

use std::sync::{Arc, OnceLock};

use fpeg::{LineIndex, Position};
use pyo3::PyTypeInfo;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedStr;
use pyo3::types::PyList;

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
    fn of(parsed: &Parsed, start: u32, end: u32) -> Meta {
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
        let children = PyList::empty(py);
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

    fn origin(&self) -> Option<(&Arc<Parsed>, u32)> {
        match &self.inner {
            Inner::View { parsed, index } => Some((parsed, *index)),
            Inner::Owned { origin, .. } => origin.as_ref().map(|(parsed, index)| (parsed, *index)),
        }
    }
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
}
