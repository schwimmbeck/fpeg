// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The core language of the semantics page: grammars after resolving, as engines run them.
//!
//! Every construct of the surface language is translated into these expressions. Skip points,
//! tokens, nodes and fields are explicit, and names are replaced by ids.

#![cfg_attr(
    not(feature = "reference"),
    expect(
        dead_code,
        reason = "only the reference interpreter reads columns and captures until the machine runs them"
    )
)]

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
    /// For each kind, the fields of its nodes with how many items each can hold.
    pub kind_fields: Vec<Vec<(FieldId, FieldCount)>>,
    /// For each kind, the value builder of the alternatives that make it.
    pub kind_builders: Vec<Option<ValueBuilder>>,
    /// For each kind, whether its items are tokens rather than nodes.
    pub kind_tokens: Vec<bool>,
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
}

/// An error label: a message or a name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Label {
    Message(String),
    Named(String),
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
    ErrorAlternative,
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
            | Expr::Label { expr, .. } => std::slice::from_ref(&**expr),
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
            | Expr::Ref { .. }
            | Expr::ErrorAlternative => &[],
        }
    }
}
