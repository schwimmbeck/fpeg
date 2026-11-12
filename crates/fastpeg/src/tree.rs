// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! The parse tree: nodes and tokens in one compact array, in postorder.
//!
//! Every consumer reads this one format. Each entry has a kind, flags, a byte span, the size
//! of its subtree and the field it fills in its parent. Children come before their parent, so
//! a bottom-up pass is one forward scan, and a node's subtree is the `size` entries ending at
//! the node. The tree is lossless: every matched byte lies in a token, except skipped text,
//! which lies in the gaps between tokens.

#![cfg_attr(
    not(feature = "reference"),
    expect(
        dead_code,
        reason = "only the reference interpreter builds trees until the engine exists"
    )
)]

use std::fmt::Write;
use std::sync::Arc;

/// Identifies a kind of node or token in a grammar's kind table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KindId(pub u32);

impl KindId {
    /// A region the parser could not match. Reserved in every kind table.
    pub const ERROR: KindId = KindId(0);
    /// A token the parser expected but did not find. Reserved in every kind table.
    pub const MISSING: KindId = KindId(1);
    /// Text the parser skipped while recovering. Reserved in every kind table.
    pub const SKIPPED: KindId = KindId(2);
    /// The number of reserved kinds at the start of every kind table.
    pub const RESERVED: u32 = 3;
}

/// Identifies a field in a grammar's field table. Field 0 means no field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FieldId(pub u16);

impl FieldId {
    /// No field.
    pub const NONE: FieldId = FieldId(0);
}

/// The names behind kind and field ids, shared by a grammar and its trees.
#[derive(Debug, Default)]
pub struct Names {
    /// Kind names, indexed by [`KindId`].
    pub kinds: Vec<String>,
    /// Field names, indexed by [`FieldId`]. Entry 0 is unused.
    pub fields: Vec<String>,
}

const TOKEN: u8 = 1;
const HIDDEN: u8 = 2;

/// A parse tree. See the module documentation for the layout.
#[derive(Clone, Debug)]
pub struct Tree {
    kinds: Vec<KindId>,
    flags: Vec<u8>,
    starts: Vec<u32>,
    ends: Vec<u32>,
    sizes: Vec<u32>,
    fields: Vec<FieldId>,
    names: Arc<Names>,
}

/// An item as a parser produces it, before the tree is laid out.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Item {
    Token {
        kind: KindId,
        field: FieldId,
        hidden: bool,
        start: u32,
        end: u32,
    },
    Node {
        kind: KindId,
        field: FieldId,
        start: u32,
        end: u32,
        children: Vec<Item>,
    },
}

impl Item {
    pub(crate) fn field(&self) -> FieldId {
        match self {
            Item::Token { field, .. } | Item::Node { field, .. } => *field,
        }
    }

    pub(crate) fn set_field(&mut self, value: FieldId) {
        match self {
            Item::Token { field, .. } | Item::Node { field, .. } => *field = value,
        }
    }

    pub(crate) fn is_hidden_token(&self) -> bool {
        matches!(self, Item::Token { hidden: true, .. })
    }

    /// The span of a node over `children`: from the first token to the last, at any depth,
    /// or empty at `at` when there is no token.
    pub(crate) fn span_of(children: &[Item], at: u32) -> (u32, u32) {
        let mut span: Option<(u32, u32)> = None;
        for child in children {
            let (start, end, has_token) = match child {
                Item::Token { start, end, .. } => (*start, *end, true),
                Item::Node {
                    start,
                    end,
                    children,
                    ..
                } => (*start, *end, contains_token(children)),
            };
            if has_token {
                span = Some(match span {
                    None => (start, end),
                    Some((first, _)) => (first, end),
                });
            }
        }
        span.unwrap_or((at, at))
    }
}

fn contains_token(items: &[Item]) -> bool {
    items.iter().any(|item| match item {
        Item::Token { .. } => true,
        Item::Node { children, .. } => contains_token(children),
    })
}

impl Tree {
    /// Lay out items, in order, as the top level of a tree.
    pub(crate) fn from_items(items: Vec<Item>, names: Arc<Names>) -> Tree {
        let mut tree = Tree {
            kinds: Vec::new(),
            flags: Vec::new(),
            starts: Vec::new(),
            ends: Vec::new(),
            sizes: Vec::new(),
            fields: Vec::new(),
            names,
        };
        for item in items {
            tree.push(item);
        }
        tree
    }

    fn push(&mut self, item: Item) -> u32 {
        match item {
            Item::Token {
                kind,
                field,
                hidden,
                start,
                end,
            } => {
                let flags = TOKEN | if hidden { HIDDEN } else { 0 };
                self.append(kind, flags, start, end, 1, field);
                1
            }
            Item::Node {
                kind,
                field,
                start,
                end,
                children,
            } => {
                let size = 1 + children.into_iter().map(|c| self.push(c)).sum::<u32>();
                self.append(kind, 0, start, end, size, field);
                size
            }
        }
    }

    fn append(&mut self, kind: KindId, flags: u8, start: u32, end: u32, size: u32, field: FieldId) {
        self.kinds.push(kind);
        self.flags.push(flags);
        self.starts.push(start);
        self.ends.push(end);
        self.sizes.push(size);
        self.fields.push(field);
    }

    /// The number of nodes and tokens.
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    /// Whether the tree has no nodes and no tokens.
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// The top-level items, in input order.
    pub fn roots(&self) -> Vec<NodeRef<'_>> {
        self.siblings_in(0, self.len())
    }

    /// The items whose subtrees exactly cover the entries from `first` to `end`.
    fn siblings_in(&self, first: usize, end: usize) -> Vec<NodeRef<'_>> {
        let mut out = Vec::new();
        let mut index = end;
        while index > first {
            let last = index - 1;
            out.push(NodeRef {
                tree: self,
                index: last,
            });
            index -= self.sizes[last] as usize;
        }
        out.reverse();
        out
    }

    /// The names of kinds and fields.
    pub fn names(&self) -> &Names {
        &self.names
    }

    /// The tree as one line of text, for tests and debugging.
    ///
    /// A node is `(kind child ...)`, a token is `KIND"text"`, a hidden token gets a `~` in
    /// front, and a field puts `name=` before the item. With `spans`, each item shows its
    /// byte span as `@start..end`.
    pub fn to_text(&self, input: &str, spans: bool) -> String {
        let mut out = String::new();
        for (i, root) in self.roots().into_iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            root.write_text(&mut out, input, spans);
        }
        out
    }
}

/// A node or token of a tree.
#[derive(Clone, Copy, Debug)]
pub struct NodeRef<'t> {
    tree: &'t Tree,
    index: usize,
}

impl<'t> NodeRef<'t> {
    /// The kind.
    pub fn kind(&self) -> KindId {
        self.tree.kinds[self.index]
    }

    /// The kind's name.
    pub fn kind_name(&self) -> &'t str {
        self.tree
            .names
            .kinds
            .get(self.kind().0 as usize)
            .map_or("", String::as_str)
    }

    /// Whether this is a token.
    pub fn is_token(&self) -> bool {
        self.tree.flags[self.index] & TOKEN != 0
    }

    /// Whether this is a hidden token.
    pub fn is_hidden(&self) -> bool {
        self.tree.flags[self.index] & HIDDEN != 0
    }

    /// The byte span in the input.
    pub fn span(&self) -> (u32, u32) {
        (self.tree.starts[self.index], self.tree.ends[self.index])
    }

    /// The field this item fills in its parent.
    pub fn field(&self) -> FieldId {
        self.tree.fields[self.index]
    }

    /// The name of the field this item fills, if any.
    pub fn field_name(&self) -> Option<&'t str> {
        match self.field() {
            FieldId::NONE => None,
            FieldId(id) => self
                .tree
                .names
                .fields
                .get(usize::from(id))
                .map(String::as_str),
        }
    }

    /// The children, hidden tokens included, in input order.
    pub fn children(&self) -> Vec<NodeRef<'t>> {
        let size = self.tree.sizes[self.index] as usize;
        self.tree.siblings_in(self.index + 1 - size, self.index)
    }

    /// The text of the span.
    pub fn text<'i>(&self, input: &'i str) -> &'i str {
        let (start, end) = self.span();
        input.get(start as usize..end as usize).unwrap_or("")
    }

    fn write_text(&self, out: &mut String, input: &str, spans: bool) {
        if let Some(field) = self.field_name() {
            out.push_str(field);
            out.push('=');
        }
        if self.is_hidden() {
            out.push('~');
        }
        let (start, end) = self.span();
        if self.is_token() {
            let name = self.kind_name();
            // An anonymous token's kind is its own text, so only named kinds are written.
            if !name.starts_with(['"', '[', '.', '\\', '%']) {
                out.push_str(name);
            }
            let _ = write!(out, "{:?}", self.text(input));
            if spans {
                let _ = write!(out, "@{start}..{end}");
            }
        } else {
            out.push('(');
            out.push_str(self.kind_name());
            if spans {
                let _ = write!(out, "@{start}..{end}");
            }
            for child in self.children() {
                out.push(' ');
                child.write_text(out, input, spans);
            }
            out.push(')');
        }
    }
}
