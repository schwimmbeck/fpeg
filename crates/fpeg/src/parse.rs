// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a parse returns when it produces no tree, and how far a parse may go.

use std::fmt;

/// Rule activations that may be nested before a parse stops.
pub const DEFAULT_MAX_DEPTH: u32 = 2_000;

/// Steps a parse may take before it stops. A step is an instruction of the parsing machine,
/// and work beyond one counts too, so that the limit bounds the time a parse takes: closing a
/// node counts its items, comparing a long literal, taking a run of a class or looking up a
/// reserved word counts its length, and testing a class beyond ASCII counts its items. A tree
/// that repeats the nodes of a remembered match or of a left recursion counts what laying out
/// the repeats takes. Ten to twenty seconds on a desktop. A parse of JSON meets the memory
/// limit first, at two to four hundred megabytes. A parse that fails runs a second time to
/// find what was expected, which takes as long again.
pub const DEFAULT_MAX_STEPS: u64 = 4_000_000_000;

/// Bytes of memory a parse may hold by default for each byte of its input.
pub const DEFAULT_MEMORY_BYTES_PER_INPUT_BYTE: u64 = 4 * 1024;

/// The least memory in bytes a parse may hold by default, whatever its input.
pub const DEFAULT_MIN_MEMORY_BYTES: u64 = 64 * 1024 * 1024;

/// The most memory in bytes a parse may hold by default, whatever its input.
pub const DEFAULT_MAX_MEMORY_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// What a parse that stopped before the end of the input expects, in an expected set.
pub(crate) const END_OF_INPUT: &str = "end of input";

/// What a failed column check expects, in an expected set.
pub(crate) const INDENTATION: &str = "indentation";

/// How far a parse may go before it stops with [`ParseErrorKind::Limit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The deepest nesting of rule activations. A rule that calls itself makes one at every
    /// level, while a small rule that does not may be compiled into its callers and make none.
    /// A fixed cap on nested matches, which keeps the reference interpreter within its own
    /// stack, stops a parse with the same limit whatever this allows.
    pub max_depth: u32,
    /// The number of steps, counted as for [`DEFAULT_MAX_STEPS`]. The reference interpreter
    /// counts steps of its own.
    pub max_steps: u64,
    /// The memory in bytes a parse may hold: what the parsing machine's stacks and tables
    /// have room for, and the tree it lays out at the end, each counted before it grows.
    /// `None` gives [`DEFAULT_MEMORY_BYTES_PER_INPUT_BYTE`] for each byte of input, at
    /// least [`DEFAULT_MIN_MEMORY_BYTES`] and at most [`DEFAULT_MAX_MEMORY_BYTES`], since a
    /// parse that runs away mostly meets a short input, and memory, unlike time, takes the
    /// process with it when it runs out. The reference interpreter does not count memory.
    pub max_memory_bytes: Option<u64>,
}

impl Limits {
    /// The memory in bytes a parse of an input of `input_bytes` may hold.
    pub fn memory_bytes(&self, input_bytes: usize) -> u64 {
        let relative = (input_bytes as u64).saturating_mul(DEFAULT_MEMORY_BYTES_PER_INPUT_BYTE);
        let default = relative.clamp(DEFAULT_MIN_MEMORY_BYTES, DEFAULT_MAX_MEMORY_BYTES);
        self.max_memory_bytes.unwrap_or(default)
    }
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_depth: DEFAULT_MAX_DEPTH,
            max_steps: DEFAULT_MAX_STEPS,
            max_memory_bytes: None,
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
#[non_exhaustive]
pub enum Limit {
    /// Too deeply nested rule activations, or matches nested deeper than the reference
    /// interpreter's stack allows. Also when the interpreter's thread, which has that stack,
    /// cannot start.
    Depth,
    /// Too many evaluation steps.
    Steps,
    /// An input too large for 32-bit offsets.
    InputSize,
    /// More memory than the limit allows.
    Memory,
}

impl ParseError {
    /// What went wrong, without the position.
    pub fn message(&self) -> String {
        match &self.kind {
            ParseErrorKind::NoMatch { expected } if expected.is_empty() => "no match".to_owned(),
            ParseErrorKind::NoMatch { expected } => format!("expected {}", expected.join(", ")),
            ParseErrorKind::Label(label) => label.clone(),
            ParseErrorKind::Limit(Limit::Depth) => {
                "rule calls nest deeper than the depth limit".to_owned()
            }
            ParseErrorKind::Limit(Limit::Steps) => {
                "the parse takes more steps than the step limit".to_owned()
            }
            ParseErrorKind::Limit(Limit::InputSize) => {
                "the input is too large for 32-bit offsets".to_owned()
            }
            ParseErrorKind::Limit(Limit::Memory) => {
                "the parse needs more memory than the memory limit allows".to_owned()
            }
            ParseErrorKind::UnknownStart(name) => format!("no rule {name} to start from"),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error at {}: {}", self.position, self.message())
    }
}

impl std::error::Error for ParseError {}
