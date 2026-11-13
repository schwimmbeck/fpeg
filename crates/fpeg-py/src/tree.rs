// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Tree`: views of the nodes of a parse, and trees built in Python.
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

/// A node of a parse tree, as Lark's `Tree`: `data` and `children`.
///
/// A tree from a parse is a view of the native tree, and its children are made when asked for.
/// `Tree(data, children)` builds one in Python.
#[pyclass(frozen, module = "fpeg", subclass)]
pub struct Tree {
    inner: Inner,
    children: OnceLock<Py<PyList>>,
}

impl Tree {
    pub fn view(parsed: Arc<Parsed>, index: u32) -> Tree {
        Tree {
            inner: Inner::View { parsed, index },
            children: OnceLock::new(),
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
    fn new(py: Python<'_>, data: Py<PyAny>, children: Bound<'_, PyAny>) -> PyResult<Tree> {
        let children = match children.cast_into::<PyList>() {
            Ok(list) => list,
            Err(error) => PyList::type_object(py)
                .call1((error.into_inner(),))?
                .cast_into::<PyList>()?,
        };
        Ok(Tree::owned(data, children.unbind(), None))
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

    /// The text of the node in its input, or `None` for a tree built in Python.
    #[getter]
    fn text(&self) -> Option<String> {
        let (parsed, index) = self.origin()?;
        let (start, end) = parsed.tree.node(index as usize)?.span();
        Some(parsed.text(start, end).to_owned())
    }
}
