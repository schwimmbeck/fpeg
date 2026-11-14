// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The engine: grammars compiled to programs for the parsing machine, and programs run over
//! inputs.

pub(crate) mod compile;
mod disasm;
mod machine;
mod profile;
mod program;
#[cfg(feature = "reference")]
mod verify;

#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::OnceLock;

use crate::diag::Diagnostic;
use crate::grammar::Grammar;
use crate::ir;
use crate::parse::{Limit, Limits, ParseError, ParseErrorKind};
use crate::tree::{Kind, Names, Tree};
use machine::{Machine, Stop};
pub use profile::{Profile, ProfileRow};
use program::Program;

/// How a grammar is compiled. The default suits every use but tests and measurements, which
/// turn single parts of the engine off or on. No setting changes what a parse returns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Which rules remember their results.
    pub memo: Memo,
    /// Which passes of the optimizer run.
    pub passes: Passes,
}

/// The passes of the optimizer, one switch each, all on by default. A parse gives the same tree
/// or error with any of them off, only its time changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Passes {
    /// Hash the keys of remembered results with one multiplication a word, not with SipHash.
    pub hasher: bool,
    /// Match a repetition of a class with one instruction, not one code point a step.
    pub span: bool,
    /// Answer a skip from where the last skip started, with its flags, at once, and take a
    /// skip expression that repeats a class as one run, without its routine.
    pub skip: bool,
}

impl Passes {
    /// Every pass, the default.
    pub const ALL: Passes = Passes {
        hasher: true,
        span: true,
        skip: true,
    };
    /// No pass: the plain machine.
    pub const NONE: Passes = Passes {
        hasher: false,
        span: false,
        skip: false,
    };
}

impl Default for Passes {
    fn default() -> Passes {
        Passes::ALL
    }
}

/// Which rules remember their results, so that a second call of a rule at a position is
/// answered at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Memo {
    /// The levels of precedence blocks, the left-recursive rules and the rules the grammar
    /// marks `@memo`, without those it marks `@nomemo`.
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
    /// The grammar as resolved, for the programs of other options.
    grammar: ir::Grammar,
    options: Options,
    /// The program without the passes, compiled on the first profile, or none when the
    /// compiler refuses it.
    plain: OnceLock<Option<Program>>,
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
    /// Compile a grammar, or report the problems in it, the first hundred at most. Columns
    /// and captures are reported as problems until the milestone of the roadmap brings them
    /// to the machine.
    pub fn new(grammar: &Grammar) -> Result<Parser, Vec<Diagnostic>> {
        Parser::with_options(grammar, &Options::default())
    }

    /// Compile a grammar as [`Parser::new`] does, with parts of the engine turned off or on.
    pub fn with_options(grammar: &Grammar, options: &Options) -> Result<Parser, Vec<Diagnostic>> {
        let resolved = crate::resolve::resolve(grammar)?;
        let program = build(&resolved, options)?;
        Ok(Parser {
            program,
            grammar: resolved,
            options: *options,
            plain: OnceLock::new(),
        })
    }

    /// The program without the passes.
    fn plain(&self) -> &Program {
        if self.options.passes == Passes::NONE {
            return &self.program;
        }
        let options = Options {
            passes: Passes::NONE,
            ..self.options
        };
        let plain = self
            .plain
            .get_or_init(|| build(&self.grammar, &options).ok());
        plain.as_ref().unwrap_or(&self.program)
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
        run(&self.program, input, start, limits, false).0
    }

    /// Parse as [`Parser::parse_with`] does and count what each rule did: how often it was
    /// called and failed, the steps spent in it, and what remembering it answered or would
    /// answer. For finding the rules that take the time. The counting slows the parse.
    ///
    /// The parse runs without the passes of the optimizer, so that every rule keeps the calls,
    /// failures and steps of the grammar as written. A parse with them takes fewer steps.
    pub fn profile(
        &self,
        input: &str,
        start: &str,
        limits: Limits,
    ) -> (Result<Tree, ParseError>, Profile) {
        let (result, profile) = run(self.plain(), input, start, limits, true);
        (result, profile.unwrap_or_default())
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

/// Compile a resolved grammar, and in test builds verify the program.
fn build(grammar: &ir::Grammar, options: &Options) -> Result<Program, Vec<Diagnostic>> {
    let program = compile::compile(grammar, options)?;
    #[cfg(feature = "reference")]
    if let Err(problem) = verify::verify(&program) {
        return Err(vec![Diagnostic {
            code: crate::diag::DiagnosticCode::Internal,
            message: format!("the compiler made a program that fails verification: {problem}"),
            span: crate::grammar::Span::default(),
        }]);
    }
    Ok(program)
}

/// Parse the whole input with a program, counting what each rule did when `profiling`.
fn run(
    program: &Program,
    input: &str,
    start: &str,
    limits: Limits,
    profiling: bool,
) -> (Result<Tree, ParseError>, Option<Profile>) {
    let refused = |kind| {
        let error = ParseError { position: 0, kind };
        (Err(error), None)
    };
    let Some(&entry) = program.entries.get(start) else {
        return refused(ParseErrorKind::UnknownStart(start.to_owned()));
    };
    if u32::try_from(input.len()).is_err() {
        return refused(ParseErrorKind::Limit(Limit::InputSize));
    }
    let mut machine = Machine::new(program, input, limits, None);
    if profiling {
        machine = machine.profiling();
    }
    let outcome = machine.run(entry);
    let profile = machine.profile();
    let farthest = machine.farthest();
    let position = farthest.unwrap_or(0) as usize;
    let result = match outcome {
        Ok(()) => machine.tree().map_err(|limit| ParseError {
            position,
            kind: ParseErrorKind::Limit(limit),
        }),
        Err(Stop::NoMatch) => {
            let expected = match farthest {
                Some(at) => {
                    let mut again = Machine::new(program, input, limits, Some(at));
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
                (program.labels.get(label as usize))
                    .cloned()
                    .unwrap_or_default(),
            ),
        }),
        Err(Stop::Limit(limit)) => Err(ParseError {
            position,
            kind: ParseErrorKind::Limit(limit),
        }),
    };
    (result, profile)
}
