// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! fpeg compiles a parsing expression grammar, given as text at runtime, to bytecode for a
//! parsing machine and builds a lossless parse tree with exact source spans.
//!
//! A [`Grammar`] is read from text or built with the [`builder`], and a [`Parser`] compiles it
//! for the machine and parses inputs into a [`Tree`]. A [`Source`] shows the errors of a parse
//! with the lines they stand in. Behind the `reference` feature, the reference interpreter
//! defines what a grammar means, and the tests compare the machine with it.
//!
//! The [documentation](https://schwimmbeck.github.io/fpeg/) describes the grammar language, its
//! diagnostics and the command line, which are the same from Rust and from Python.
//!
//! ```
//! use fpeg::{Grammar, Parser};
//!
//! let text = "start: first=N (\"+\" N)*\nN: [0-9]+\n%ignore \" \"+";
//! let grammar = Grammar::from_text(text).expect("the grammar reads");
//! let parser = Parser::new(&grammar).expect("the grammar compiles");
//! let input = "1 + 23";
//! let tree = parser.parse(input, "start").expect("the input parses");
//! assert_eq!(tree.to_text(input, false), "(start first=N\"1\" ~\"+\" N\"23\")");
//! ```

// The core returns errors as values and never panics on any grammar or input.
#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod analysis;
mod check;
mod diag;
mod grammar;
mod graph;
mod ir;
mod items;
mod lines;
mod parse;
#[cfg(feature = "reference")]
mod reference;
mod render;
mod resolve;
mod tree;
mod vm;

pub use check::check;
pub use diag::{Diagnostic, DiagnosticCode, Severity};
pub use grammar::builder;
pub use grammar::{
    Alternative, BUILT_IN_BUILDERS, BuilderArg, BuilderValue, Class, ClassItem, Constant, Count,
    ErrorLabel, Expr, ExprKind, Grammar, Import, Level, MAX_EXPRESSION_DEPTH, ModuleFiles, Modules,
    Name, Property, Recover, Rule, RuleBody, RuleKind, Span, Text, ValueBuilder,
};
pub use lines::{LineCursor, LineIndex, Position};
pub use parse::{
    DEFAULT_MAX_DEPTH, DEFAULT_MAX_MEMORY_BYTES, DEFAULT_MAX_STEPS,
    DEFAULT_MEMORY_BYTES_PER_INPUT_BYTE, DEFAULT_MIN_MEMORY_BYTES, Limit, Limits, ParseError,
    ParseErrorKind, Recovery,
};
#[cfg(feature = "reference")]
pub use reference::Reference;
pub use render::{Source, error_title, visible};
pub use tree::{FieldCount, FieldId, Kind, KindId, Names, NodeRef, Tree};
pub use vm::{Memo, Options, Parser, Passes, Profile, ProfileRow};

/// The version of this crate, which the command line and, in Python's spelling, the Python
/// package report.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
