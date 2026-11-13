// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The engine: grammars compiled to programs for the parsing machine, and programs run over
//! inputs.

mod compile;
mod disasm;
mod machine;
mod program;
#[cfg(feature = "reference")]
mod verify;

#[cfg(test)]
mod tests;

use std::fmt;

use crate::grammar::{Grammar, GrammarError};
use crate::parse::{Limit, Limits, ParseError, ParseErrorKind};
use crate::tree::{Kind, Names, Tree};
use machine::{Machine, Stop};
use program::Program;

/// How a grammar is compiled. The default suits every use but tests and measurements, which
/// turn single parts of the engine off or on. No setting changes what a parse returns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Which rules remember their results.
    pub memo: Memo,
}

/// Which rules remember their results, so that a second call of a rule at a position is
/// answered at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Memo {
    /// The rules the grammar marks `@memo`.
    #[default]
    Grammar,
    /// No rule.
    Off,
    /// Every rule.
    All,
}

/// A grammar compiled for the parsing machine.
///
/// A parser is immutable, so one parser serves any number of parses, from any thread.
///
/// ```
/// use fpeg::builder::*;
/// use fpeg::Parser;
///
/// let mut g = GrammarBuilder::new();
/// g.rule("start", sep_some(r("NUMBER"), lit("+")));
/// g.rule("NUMBER", some(class([range('0', '9')])));
/// let parser = Parser::new(&g.build()).expect("the grammar compiles");
/// let tree = parser.parse("1+20", "start").expect("the input parses");
/// assert_eq!(tree.to_text("1+20", false), "(start NUMBER\"1\" ~\"+\" NUMBER\"20\")");
/// ```
pub struct Parser {
    program: Program,
}

// Not derived: the program would print every instruction.
impl fmt::Debug for Parser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Parser")
            .field("rules", &self.program.rules.len())
            .field("instructions", &self.program.code.len())
            .finish_non_exhaustive()
    }
}

impl Parser {
    /// Compile a grammar, or report the problems in it, the first hundred at most. Left
    /// recursion, columns and captures are reported as problems until the milestones of the
    /// roadmap bring them to the machine.
    pub fn new(grammar: &Grammar) -> Result<Parser, Vec<GrammarError>> {
        Parser::with_options(grammar, &Options::default())
    }

    /// Compile a grammar as [`Parser::new`] does, with parts of the engine turned off or on.
    pub fn with_options(grammar: &Grammar, options: &Options) -> Result<Parser, Vec<GrammarError>> {
        let resolved = crate::resolve::resolve(grammar)?;
        let program = compile::compile(&resolved, options)?;
        #[cfg(feature = "reference")]
        if let Err(problem) = verify::verify(&program) {
            return Err(vec![GrammarError {
                code: crate::diag::DiagnosticCode::Internal,
                message: format!("the compiler made a program that fails verification: {problem}"),
                span: crate::grammar::Span::default(),
            }]);
        }
        Ok(Parser { program })
    }

    /// Parse the whole input from the rule `start`, with the default limits.
    pub fn parse(&self, input: &str, start: &str) -> Result<Tree, ParseError> {
        self.parse_with(input, start, Limits::default())
    }

    /// Parse the whole input from the rule `start`.
    ///
    /// A parse that fails is run a second time to collect what was expected at the farthest
    /// failure, so a valid input never pays for it.
    pub fn parse_with(&self, input: &str, start: &str, limits: Limits) -> Result<Tree, ParseError> {
        let Some(&entry) = self.program.entries.get(start) else {
            return Err(ParseError {
                position: 0,
                kind: ParseErrorKind::UnknownStart(start.to_owned()),
            });
        };
        if u32::try_from(input.len()).is_err() {
            return Err(ParseError {
                position: 0,
                kind: ParseErrorKind::Limit(Limit::InputSize),
            });
        }
        let mut machine = Machine::new(&self.program, input, limits, None);
        let outcome = machine.run(entry);
        let farthest = machine.farthest();
        let position = farthest.unwrap_or(0) as usize;
        match outcome {
            Ok(()) => machine.tree().map_err(|limit| ParseError {
                position,
                kind: ParseErrorKind::Limit(limit),
            }),
            Err(Stop::NoMatch) => {
                let expected = match farthest {
                    Some(at) => {
                        let mut again = Machine::new(&self.program, input, limits, Some(at));
                        // The second run follows the first exactly, so it ends the same way.
                        let _ = again.run(entry);
                        again.expected()
                    }
                    None => Vec::new(),
                };
                Err(ParseError {
                    position,
                    kind: ParseErrorKind::NoMatch { expected },
                })
            }
            Err(Stop::Error { label, position }) => Err(ParseError {
                position: position as usize,
                kind: ParseErrorKind::Label(
                    (self.program.labels.get(label as usize))
                        .cloned()
                        .unwrap_or_default(),
                ),
            }),
            Err(Stop::Limit(limit)) => Err(ParseError {
                position,
                kind: ParseErrorKind::Limit(limit),
            }),
        }
    }

    /// The names of the grammar's kinds and fields, which its trees share.
    pub fn names(&self) -> &Names {
        &self.program.names
    }

    /// The kind table, indexed by [`KindId`].
    pub fn kinds(&self) -> &[Kind] {
        &self.program.kinds
    }

    /// Whether a parse from `start` always leaves exactly one visible item at the top, or
    /// `None` when `start` cannot start a parse. A rule that makes a node leaves one, and so does
    /// a transparent rule. A rule `_name` or a level of a precedence block may leave none or
    /// several.
    pub fn single_item(&self, start: &str) -> Option<bool> {
        self.program.single_item.get(start).copied()
    }

    /// The compiled program as text, one instruction per line. Its format may change with any
    /// release. For tests and for debugging the compiler.
    #[cfg(feature = "reference")]
    pub fn disassemble(&self) -> String {
        self.program.to_string()
    }
}
