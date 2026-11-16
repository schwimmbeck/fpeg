// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The parse tree: nodes and tokens in one compact array, in postorder.
//!
//! Every consumer reads this one format. Each entry has a kind, flags, a byte span, the size
//! of its subtree and the field it fills in its parent. Children come before their parent, so
//! a bottom-up pass is one forward scan, and a node's subtree is the `size` entries ending at
//! the node. The tree is lossless: every matched byte lies in a token, except skipped text,
//! which lies in the gaps between tokens.

use std::fmt::Write;
use std::sync::Arc;

use crate::grammar::ValueBuilder;
use crate::parse::Recovery;

/// Identifies a kind of node or token in a grammar's kind table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KindId(pub u32);

impl KindId {
    /// A region the parser could not match. Reserved in every kind table.
    pub const ERROR: KindId = KindId(0);
    /// A token the parser expected but did not find. Reserved in every kind table.
    pub const MISSING: KindId = KindId(1);
    /// Reserved in every kind table for text a parse skips while recovering, which no parse
    /// makes yet.
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

/// How many items a field holds in one node. The variants are ordered from the fewest cases to
/// the most, so that the larger of two counts covers both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FieldCount {
    /// Exactly one.
    One,
    /// One or none.
    Optional,
    /// Possibly several, or none.
    Many,
}

/// What a compiled grammar knows about one kind of node or token.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Kind {
    /// The name, shared by every kind that one rule or alias makes.
    pub name: String,
    /// What an expected set shows for it.
    pub display: String,
    /// The fields of its nodes, by field id, with how many items each can hold.
    pub fields: Vec<(FieldId, FieldCount)>,
    /// The value builder of the alternatives that make it, or of its terminal.
    pub builder: Option<ValueBuilder>,
    /// Whether its items are tokens. A kind is of tokens or of nodes, never both: the
    /// tokens of a terminal and the nodes of an alias of the same name have kinds apart.
    pub token: bool,
    /// Whether a parse can make items of it, in normal or in recovering mode. Not so for a
    /// transparent rule whose every match leaves exactly one visible item, which takes the
    /// node's place, nor for `<error>` and `<missing>` in a grammar without `%recover`, nor ever
    /// for `<skipped>`.
    pub made: bool,
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

/// The bytes an entry takes in the columns of a tree.
pub(crate) const ENTRY_BYTES: u64 =
    (size_of::<KindId>() + size_of::<u8>() + 3 * size_of::<u32>() + size_of::<FieldId>()) as u64;

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
    recoveries: Vec<Recovery>,
}

impl Tree {
    /// A tree without entries, for an engine to append to in postorder.
    pub(crate) fn empty(names: Arc<Names>) -> Tree {
        Tree {
            kinds: Vec::new(),
            flags: Vec::new(),
            starts: Vec::new(),
            ends: Vec::new(),
            sizes: Vec::new(),
            fields: Vec::new(),
            names,
            recoveries: Vec::new(),
        }
    }

    /// Record what a parse in recovering mode recovered from, in the order of the text.
    pub(crate) fn set_recoveries(&mut self, recoveries: Vec<Recovery>) {
        self.recoveries = recoveries;
    }

    /// The labeled errors the parse recovered from, in the order of the text, each standing in
    /// the tree as an `<error>` node or a `<missing>` token. Empty after a parse in normal mode.
    pub fn recoveries(&self) -> &[Recovery] {
        &self.recoveries
    }

    /// Make room for `entries` more entries in every column, so that appending them moves none.
    pub(crate) fn reserve(&mut self, entries: usize) {
        self.kinds.reserve(entries);
        self.flags.reserve(entries);
        self.starts.reserve(entries);
        self.ends.reserve(entries);
        self.sizes.reserve(entries);
        self.fields.reserve(entries);
    }

    /// Append a token.
    pub(crate) fn push_token(
        &mut self,
        kind: KindId,
        hidden: bool,
        start: u32,
        end: u32,
        field: FieldId,
    ) {
        let flags = TOKEN | if hidden { HIDDEN } else { 0 };
        self.append(kind, flags, start, end, 1, field);
    }

    /// Append a node whose subtree, itself included, is the last `size` entries.
    pub(crate) fn push_node(
        &mut self,
        kind: KindId,
        start: u32,
        end: u32,
        size: u32,
        field: FieldId,
    ) {
        self.append(kind, 0, start, end, size, field);
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

    /// The node or token at a position of the postorder, or `None` past the end.
    pub fn node(&self, index: usize) -> Option<NodeRef<'_>> {
        (index < self.len()).then_some(NodeRef { tree: self, index })
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
        /// What is left to write: an item, with a space before it or not, or a closing bracket.
        enum Step<'t> {
            Item(NodeRef<'t>, bool),
            Close,
        }
        // An explicit stack, since trees from left recursion are as deep as they are long.
        let mut pending: Vec<Step<'_>> = Vec::new();
        for (i, root) in self.roots().into_iter().enumerate().rev() {
            pending.push(Step::Item(root, i > 0));
        }
        let mut out = String::new();
        while let Some(step) = pending.pop() {
            match step {
                Step::Close => out.push(')'),
                Step::Item(item, space) => {
                    if space {
                        out.push(' ');
                    }
                    item.write_head(&mut out, input, spans, true);
                    if !item.is_token() {
                        pending.push(Step::Close);
                        for child in item.children().into_iter().rev() {
                            pending.push(Step::Item(child, true));
                        }
                    }
                }
            }
        }
        out
    }
}

impl Tree {
    /// The tree with one item per line, written as in [`Tree::to_text`], each line indented by
    /// two spaces for every node it lies in.
    pub fn to_lines(&self, input: &str, spans: bool) -> String {
        let mut out = String::new();
        // An explicit stack, since trees from left recursion are as deep as they are long.
        let mut pending: Vec<(NodeRef<'_>, usize)> = self
            .roots()
            .into_iter()
            .rev()
            .map(|root| (root, 0))
            .collect();
        while let Some((item, depth)) = pending.pop() {
            for _ in 0..depth {
                out.push_str("  ");
            }
            item.write_head(&mut out, input, spans, false);
            for child in item.children().into_iter().rev() {
                pending.push((child, depth + 1));
            }
            out.push('\n');
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
    /// The position in the postorder, which [`Tree::node`] takes.
    pub fn index(&self) -> usize {
        self.index
    }

    /// The number of entries of the subtree, this one included. They are the entries right
    /// before this one and this one.
    pub fn size(&self) -> usize {
        self.tree.sizes[self.index] as usize
    }

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

    /// Write a token, or a node up to its children, opening the node's bracket if `bracket`.
    fn write_head(&self, out: &mut String, input: &str, spans: bool, bracket: bool) {
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
        } else {
            if bracket {
                out.push('(');
            }
            out.push_str(self.kind_name());
        }
        if spans {
            let _ = write!(out, "@{start}..{end}");
        }
    }
}
