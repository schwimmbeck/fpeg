// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! The core language of the semantics page: grammars after resolving, as engines run them.
//!
//! Every construct of the surface language is translated into these expressions. Skip points,
//! tokens, nodes and fields are explicit, and names are replaced by ids.

#![cfg_attr(
    not(feature = "reference"),
    expect(
        dead_code,
        reason = "only the reference interpreter runs the core language until the engine exists"
    )
)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::grammar::ClassItem;
use crate::tree::{FieldId, KindId, Names};

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
    /// every rule except templates and precedence levels.
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
}

/// A resolved rule. Its name and kind live on in the expressions that call it.
#[derive(Debug)]
pub(crate) struct Rule {
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
#[derive(Clone, Debug, PartialEq)]
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
    },
    Ref {
        capture: CaptureId,
    },
    Throw(LabelId),
    ErrorAlternative,
}
