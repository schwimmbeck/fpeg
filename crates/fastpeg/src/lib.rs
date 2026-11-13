// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! fastpeg compiles a parsing expression grammar, given as text at runtime, to bytecode for a
//! parsing machine and builds a lossless parse tree with exact source spans.
//!
//! The crate holds no parser yet. It has the grammar model with its [`builder`], the parse tree,
//! and, behind the `reference` feature, the reference interpreter that defines what a grammar
//! means.

// The core returns errors as values and never panics on any grammar or input.
#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod grammar;
mod ir;
mod lines;
mod parse;
#[cfg(feature = "reference")]
mod reference;
mod resolve;
mod tree;

pub use grammar::builder;
pub use grammar::{
    Alternative, BuilderArg, BuilderValue, Class, ClassItem, Constant, Count, ErrorLabel, Expr,
    ExprKind, Grammar, GrammarError, Import, Level, MAX_EXPRESSION_DEPTH, Name, Property, Recover,
    Rule, RuleBody, RuleKind, Span, Text, ValueBuilder,
};
pub use lines::LineIndex;
pub use parse::{DEFAULT_MAX_DEPTH, DEFAULT_MAX_STEPS, Limit, Limits, ParseError, ParseErrorKind};
#[cfg(feature = "reference")]
pub use reference::Reference;
pub use tree::{FieldId, KindId, Names, NodeRef, Tree};

/// The version of this crate, which the command line and the Python package report.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
