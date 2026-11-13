// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! fpeg compiles a parsing expression grammar, given as text at runtime, to bytecode for a
//! parsing machine and builds a lossless parse tree with exact source spans.
//!
//! A [`Grammar`] is read from text or built with the [`builder`], and a [`Parser`] compiles it
//! for the machine and parses inputs into a [`Tree`]. The machine does not run left recursion,
//! columns and captures yet. Behind the `reference` feature, the reference interpreter defines
//! what a grammar means, and the tests compare the machine with it.

// The core returns errors as values and never panics on any grammar or input.
#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod analysis;
mod diag;
mod grammar;
mod ir;
mod items;
mod lines;
mod parse;
#[cfg(feature = "reference")]
mod reference;
mod resolve;
mod tree;
mod vm;

pub use diag::DiagnosticCode;
pub use grammar::builder;
pub use grammar::{
    Alternative, BUILT_IN_BUILDERS, BuilderArg, BuilderValue, Class, ClassItem, Constant, Count,
    ErrorLabel, Expr, ExprKind, Grammar, GrammarError, Import, Level, MAX_EXPRESSION_DEPTH, Name,
    Property, Recover, Rule, RuleBody, RuleKind, Span, Text, ValueBuilder,
};
pub use lines::{LineCursor, LineIndex, Position};
pub use parse::{DEFAULT_MAX_DEPTH, DEFAULT_MAX_STEPS, Limit, Limits, ParseError, ParseErrorKind};
#[cfg(feature = "reference")]
pub use reference::Reference;
pub use tree::{FieldCount, FieldId, Kind, KindId, Names, NodeRef, Tree};
pub use vm::{Memo, Options, Parser};

/// The version of this crate, which the command line and the Python package report.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
