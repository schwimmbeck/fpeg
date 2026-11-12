// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! fastpeg compiles a parsing expression grammar, given as text at runtime, to bytecode for a
//! parsing machine and builds a lossless parse tree with exact source spans.
//!
//! The crate holds no parser yet. It has the grammar model with its builder, the parse tree,
//! and, behind the `reference` feature, the reference interpreter that defines what a grammar
//! means.

// The core returns errors as values and never panics on any grammar or input.
#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod grammar;
pub(crate) mod ir;
pub mod lines;
#[cfg(feature = "reference")]
pub mod reference;
pub mod tree;

/// The version of this crate, which the command line and the Python package report.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
