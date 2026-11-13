// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a parse returns when it produces no tree, and how far a parse may go.

use std::fmt;

/// Rule activations that may be nested before a parse stops.
pub const DEFAULT_MAX_DEPTH: u32 = 2_000;

/// Steps a parse may take before it stops. A step is an instruction of the parsing machine,
/// and work beyond one counts too, so that the limit bounds the time a parse takes: closing a
/// node counts its items, and looking up a reserved word counts its length. About ten seconds
/// on a desktop, enough for several hundred megabytes of JSON.
pub const DEFAULT_MAX_STEPS: u64 = 4_000_000_000;

/// What a parse that stopped before the end of the input expects, in an expected set.
pub(crate) const END_OF_INPUT: &str = "end of input";

/// How far a parse may go before it stops with [`ParseErrorKind::Limit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The deepest nesting of rule activations. A fixed cap on nested matches, which keeps the
    /// reference interpreter within its own stack, stops a parse with the same limit whatever
    /// this allows.
    pub max_depth: u32,
    /// The number of steps, counted as for [`DEFAULT_MAX_STEPS`]. The reference interpreter
    /// counts steps of its own.
    pub max_steps: u64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_depth: DEFAULT_MAX_DEPTH,
            max_steps: DEFAULT_MAX_STEPS,
        }
    }
}

/// Why a parse did not produce a tree.
#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    /// The byte offset the error refers to.
    pub position: usize,
    /// What went wrong.
    pub kind: ParseErrorKind,
}

/// The kinds of parse errors.
#[derive(Clone, Debug, PartialEq)]
pub enum ParseErrorKind {
    /// The input does not match. `expected` holds what failed at the farthest position.
    NoMatch {
        /// What the grammar expected there, sorted.
        expected: Vec<String>,
    },
    /// A labeled error occurred.
    Label(String),
    /// The parse hit a limit.
    Limit(Limit),
    /// The start rule does not exist or cannot start a parse.
    UnknownStart(String),
}

/// The limits a parse can hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    /// Too deeply nested rule activations, or matches nested deeper than the reference
    /// interpreter's stack allows. Also when the interpreter's thread, which has that stack,
    /// cannot start.
    Depth,
    /// Too many evaluation steps.
    Steps,
    /// An input too large for 32-bit offsets.
    InputSize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error at {}: ", self.position)?;
        match &self.kind {
            ParseErrorKind::NoMatch { expected } if expected.is_empty() => f.write_str("no match"),
            ParseErrorKind::NoMatch { expected } => write!(f, "expected {}", expected.join(", ")),
            ParseErrorKind::Label(label) => f.write_str(label),
            ParseErrorKind::Limit(limit) => write!(f, "{limit:?} limit exceeded"),
            ParseErrorKind::UnknownStart(name) => write!(f, "no rule {name} to start from"),
        }
    }
}

impl std::error::Error for ParseError {}
