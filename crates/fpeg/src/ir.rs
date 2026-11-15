// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The core language of the semantics page: grammars after resolving, as engines run them.
//!
//! Every construct of the surface language is translated into these expressions. Skip points,
//! tokens, nodes and fields are explicit, and names are replaced by ids.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::grammar::{ClassItem, Span, ValueBuilder};
use crate::tree::{FieldCount, FieldId, Kind, KindId, Names};

/// Identifies a rule of a resolved grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RuleId(pub u32);

/// Identifies a declared flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct FlagId(pub u16);

/// Identifies a capture name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CaptureId(pub u16);

/// Identifies an error label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LabelId(pub u32);

/// A resolved grammar.
#[derive(Debug)]
pub(crate) struct Grammar {
    pub rules: Vec<Rule>,
    /// For every rule that can start a parse, by name, a reference to it in syntactic mode:
    /// every rule except templates, and every level of a precedence block.
    pub entry: HashMap<String, Expr>,
    pub names: Arc<Names>,
    /// What an expected set shows for each kind: the display name, the name, or the literal.
    pub kind_display: Vec<String>,
    /// The skip expression in plain mode, if the grammar has `%ignore`.
    pub skip: Option<Expr>,
    pub word: CharSet,
    pub reserved: Reserved,
    pub tab_width: u32,
    pub labels: Vec<Label>,
    /// How many error alternatives the grammar has. A failed parse is explained where it has
    /// one.
    pub error_alternatives: u32,
    /// The rank of each error alternative in the order of the text, by index, which orders the
    /// attempts at one position. A module's text comes after the text that uses it, and the
    /// instances of a rule come in the order they were made.
    pub error_ranks: Vec<u32>,
    /// The recovery of each label, by label id, where `%recover` gives it one: the expression a
    /// parse in recovering mode matches where the label fails.
    pub recoveries: Vec<Option<Expr>>,
    /// For each kind, the fields of its nodes with how many items each can hold.
    pub kind_fields: Vec<Vec<(FieldId, FieldCount)>>,
    /// For each kind, the value builder of the alternatives that make it.
    pub kind_builders: Vec<Option<ValueBuilder>>,
    /// For each kind, whether its items are tokens rather than nodes.
    pub kind_tokens: Vec<bool>,
    /// For each kind, whether a parse can make items of it.
    pub kind_made: Vec<bool>,
    /// For each rule that can start a parse, whether it always leaves one visible item.
    pub single_item: HashMap<String, bool>,
}

impl Grammar {
    /// The kind table that engines show to their callers.
    pub fn kinds(&self) -> Vec<Kind> {
        (self.names.kinds.iter().enumerate())
            .map(|(id, name)| Kind {
                name: name.clone(),
                display: self.kind_display.get(id).cloned().unwrap_or_default(),
                fields: self.kind_fields.get(id).cloned().unwrap_or_default(),
                builder: self.kind_builders.get(id).cloned().flatten(),
                token: self.kind_tokens.get(id).copied().unwrap_or(false),
                made: self.kind_made.get(id).copied().unwrap_or(false),
            })
            .collect()
    }
}

/// A resolved rule. Its kind lives on in the expressions that call it.
#[derive(Debug)]
pub(crate) struct Rule {
    /// The name as written. The instances of a template share it, and a level has its own.
    pub name: String,
    /// Where the name stands in the grammar source.
    pub span: Span,
    pub body: Expr,
    pub memo: MemoMark,
}

/// What a grammar says about remembering the results of a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemoMark {
    /// Nothing: the engine decides.
    None,
    /// `@memo`.
    Memo,
    /// `@nomemo`.
    NoMemo,
}

/// An error label: a message or a name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Label {
    Message(String),
    Named(String),
}

impl Label {
    /// The message or the name.
    pub fn text(&self) -> &str {
        match self {
            Label::Message(text) | Label::Named(text) => text,
        }
    }
}

/// The reserved words, compared exactly or, for case-insensitive literals, by ASCII lowercase.
#[derive(Debug, Default)]
pub(crate) struct Reserved {
    pub exact: HashSet<String>,
    pub folded: HashSet<String>,
}

impl Reserved {
    pub fn contains(&self, text: &str) -> bool {
        self.exact.contains(text) || self.folded.contains(&text.to_ascii_lowercase())
    }
}

/// A set of code points: ranges and properties, possibly negated.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CharSet {
    pub negated: bool,
    pub items: Vec<ClassItem>,
}

impl CharSet {
    pub fn contains(&self, c: char) -> bool {
        let listed = self.items.iter().any(|item| match *item {
            ClassItem::Range(first, last) => first <= c && c <= last,
            ClassItem::Property { property, negated } => property.contains(c) != negated,
        });
        listed != self.negated
    }
}

/// A core expression. See the semantics page for the meaning of each.
#[derive(Clone, Debug)]
pub(crate) enum Expr {
    Empty,
    Literal {
        text: Box<str>,
        case_insensitive: bool,
    },
    Class(Box<CharSet>),
    Any,
    Call(RuleId),
    Seq(Vec<Expr>),
    Choice(Vec<Expr>),
    Repeat {
        expr: Box<Expr>,
        min: u32,
        max: Option<u32>,
    },
    And(Box<Expr>),
    Not(Box<Expr>),
    Cut,
    Skip,
    Token {
        kind: KindId,
        hidden: bool,
        expr: Box<Expr>,
    },
    Node {
        kind: KindId,
        expr: Box<Expr>,
    },
    Transparent {
        kind: KindId,
        expr: Box<Expr>,
    },
    Field {
        field: FieldId,
        expr: Box<Expr>,
    },
    WordEnd,
    Unreserved(Box<Expr>),
    FlagOn {
        flag: FlagId,
        expr: Box<Expr>,
    },
    FlagOff {
        flag: FlagId,
        expr: Box<Expr>,
    },
    FlagTest {
        flag: FlagId,
        negated: bool,
    },
    Indent {
        expr: Box<Expr>,
        skip: bool,
    },
    Same {
        skip: bool,
    },
    /// `!.`: the end of the input, after the skip where `skip` holds.
    End {
        skip: bool,
    },
    Def {
        capture: CaptureId,
        expr: Box<Expr>,
        skip: bool,
    },
    Ref {
        capture: CaptureId,
    },
    Label {
        label: LabelId,
        expr: Box<Expr>,
        skip: bool,
    },
    /// `%error(message)` after the parts of a sequence, `expr`: it fails, and where a failed
    /// parse is explained, its attempts are tested. `index` names it in the tables of the
    /// grammar, and its rank in [`Grammar::error_ranks`] orders it among the others.
    ErrorAlternative {
        expr: Box<Expr>,
        message: String,
        index: u32,
    },
    /// An expression that the translation uses twice, the item of a separated repetition,
    /// shared so that nesting them does not double the grammar at every level.
    Shared(Arc<Expr>),
}

impl Expr {
    /// The direct subexpressions. A pass that follows them into shared parts visits each shared
    /// part at every use, so passes keep a set of those they have seen.
    pub fn children(&self) -> &[Expr] {
        match self {
            Expr::Seq(items) | Expr::Choice(items) => items,
            Expr::Repeat { expr, .. }
            | Expr::And(expr)
            | Expr::Not(expr)
            | Expr::Token { expr, .. }
            | Expr::Node { expr, .. }
            | Expr::Transparent { expr, .. }
            | Expr::Field { expr, .. }
            | Expr::Unreserved(expr)
            | Expr::FlagOn { expr, .. }
            | Expr::FlagOff { expr, .. }
            | Expr::Indent { expr, .. }
            | Expr::Def { expr, .. }
            | Expr::Label { expr, .. }
            | Expr::ErrorAlternative { expr, .. } => std::slice::from_ref(&**expr),
            Expr::Shared(expr) => std::slice::from_ref(&**expr),
            Expr::Empty
            | Expr::Literal { .. }
            | Expr::Class(_)
            | Expr::Any
            | Expr::Call(_)
            | Expr::Cut
            | Expr::Skip
            | Expr::WordEnd
            | Expr::FlagTest { .. }
            | Expr::Same { .. }
            | Expr::End { .. }
            | Expr::Ref { .. } => &[],
        }
    }
}
