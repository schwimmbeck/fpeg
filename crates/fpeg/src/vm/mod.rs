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
use crate::parse::{Found, Limit, Limits, ParseError, ParseErrorKind, Recovery};
use crate::tree::{Kind, Names, Tree};
use machine::{Machine, Stop};
pub use profile::{Profile, ProfileRow};
use program::Program;

/// Steps a parse takes at least before it starts again with every rule remembered.
const RESTART_MIN_STEPS: u64 = 1_000_000;

/// Steps a parse takes for each byte of input before it starts again with every rule
/// remembered, where they come to more than [`RESTART_MIN_STEPS`].
const RESTART_STEPS_PER_BYTE: u64 = 1_000;

/// How a grammar is compiled. The default suits every use but tests and measurements, which
/// turn single parts of the engine off or on. No setting changes what a parse returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Which rules remember their results.
    pub memo: Memo,
    /// Which passes of the optimizer run.
    pub passes: Passes,
    /// Whether a parse that runs out of its budget of steps starts again with every rule
    /// remembered, for the steps the limit leaves. The budget is a million steps, or a
    /// thousand for each byte of input where that is more. Remembering never changes a
    /// result, and it bounds the time of a grammar that repeats work no rule it remembers
    /// by default saves. Where remembering every rule needs more memory or deeper nesting
    /// than the limits allow, the parse runs once more as it started, for the steps left.
    /// The steps of the runs before count against the limit, so a parse that needs nearly
    /// all of it may reach it.
    pub restart: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            memo: Memo::default(),
            passes: Passes::default(),
            restart: true,
        }
    }
}

/// The passes of the optimizer, one switch each, all on by default. A parse gives the same tree
/// or error with any of them off, only its time changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Passes {
    /// Hash the keys of remembered results with one multiplication a word, under keys drawn
    /// for each parse, not with SipHash.
    pub hasher: bool,
    /// Match a repetition of a class with one instruction, not one code point a step.
    pub span: bool,
    /// Answer a skip from where the last skip started, with its flags and context, at once,
    /// end a skip at a byte the skip expression cannot start with without its routine, and take
    /// a skip expression that repeats a class as one run.
    pub skip: bool,
    /// Test the next byte before an alternative, an optional expression or a step of a loop,
    /// and jump past it where it cannot start with that byte.
    pub dispatch: bool,
    /// Compile a call of a small rule as the rule's body, so that it takes no call and no
    /// return, and no activation of the depth limit.
    pub inline: bool,
    /// Match `(c | e)*`, where `c` is a class, as `c* (e c*)*`, so that each run of the class
    /// takes one instruction.
    pub split: bool,
    /// Run a directly left-recursive rule of the right shape as a loop over its seed, in
    /// place of the growing loop with its frame and a second run of the base.
    pub left_loop: bool,
}

impl Passes {
    /// Every pass, the default.
    pub const ALL: Passes = Passes {
        hasher: true,
        span: true,
        skip: true,
        dispatch: true,
        inline: true,
        split: true,
        left_loop: true,
    };
    /// No pass: the plain machine.
    pub const NONE: Passes = Passes {
        hasher: false,
        span: false,
        skip: false,
        dispatch: false,
        inline: false,
        split: false,
        left_loop: false,
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
    /// The rules that a choice can try twice at one position, through the starts of two of
    /// its alternatives, the left-recursive rules that do not run as a loop, and the rules
    /// the grammar marks `@memo`, without those it marks `@nomemo`.
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
    /// The programs of other options, each compiled when first needed, or none where the
    /// compiler refuses it: without the passes for a profile, and with every rule
    /// remembered for a restart, also without the passes. See [`Parser::program_for`].
    plain: OnceLock<Option<Program>>,
    remembering: OnceLock<Option<Program>>,
    plain_remembering: OnceLock<Option<Program>>,
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
    /// Compile a grammar, or report the problems in it, the first hundred at most.
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
            remembering: OnceLock::new(),
            plain_remembering: OnceLock::new(),
        })
    }

    /// The program without the passes where `plain`, and with every rule remembered where
    /// `remembering`. Without the passes, a rule that the parse runs as a loop runs as one
    /// still: kept as the growing loop, it would be remembered, and would answer calls that
    /// the parse repeats.
    fn program_for(&self, plain: bool, remembering: bool) -> &Program {
        let options = Options {
            memo: if remembering {
                Memo::All
            } else {
                self.options.memo
            },
            passes: if plain {
                Passes {
                    left_loop: self.options.passes.left_loop,
                    ..Passes::NONE
                }
            } else {
                self.options.passes
            },
            ..self.options
        };
        let lock = match (plain, remembering) {
            _ if options == self.options => return &self.program,
            (true, false) => &self.plain,
            (false, true) => &self.remembering,
            _ => &self.plain_remembering,
        };
        let program = lock.get_or_init(|| build(&self.grammar, &options).ok());
        program.as_ref().unwrap_or(&self.program)
    }

    /// Parse with the program without the passes where `plain`. A parse that runs out of
    /// its budget of steps starts again with every rule remembered, for the steps the limit
    /// leaves, and its profile counts every run.
    fn run(
        &self,
        plain: bool,
        input: &str,
        start: &str,
        limits: Limits,
        how: Run,
    ) -> (Result<Tree, ParseError>, Option<Profile>) {
        let program = self.program_for(plain, false);
        let budget = (input.len() as u64)
            .saturating_mul(RESTART_STEPS_PER_BYTE)
            .max(RESTART_MIN_STEPS);
        let restarts = self.options.restart && self.options.memo != Memo::All;
        let budget = if restarts { budget } else { limits.max_steps };
        let first = attempt(program, input, start, limits, budget, how);
        if let Some(result) = first.result {
            return (result, first.profile);
        }
        let left = |steps: u64| Limits {
            max_steps: limits.max_steps.saturating_sub(steps),
            ..limits
        };
        let again = self.program_for(plain, true);
        let second = attempt(again, input, start, left(first.steps), u64::MAX, how);
        let mut profile = joined(first.profile, second.profile);
        if let Some(profile) = &mut profile {
            profile.restarts += 1;
        }
        let result = match second.result {
            // Remembering every rule holds more memory, and with no rule compiled into its
            // callers makes more activations, so the first program goes on once more.
            Some(Err(ParseError {
                kind: ParseErrorKind::Limit(Limit::Memory | Limit::Depth),
                ..
            })) => {
                let steps = first.steps + second.steps;
                let third = attempt(program, input, start, left(steps), u64::MAX, how);
                profile = joined(profile, third.profile);
                third.result
            }
            result => result,
        };
        // With no budget below the limit, a run after the first cannot run out of one.
        let result = result.unwrap_or(Err(ParseError {
            position: 0,
            kind: ParseErrorKind::Limit(Limit::Steps),
            recoveries: Vec::new(),
        }));
        (result, profile)
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
        self.run(false, input, start, limits, Run::default()).0
    }

    /// Parse in recovering mode, with the default limits: where a label that `%recover` gives
    /// a recovery fails, an error item takes the place of what failed, and the parse goes on.
    /// The tree lists what the parse recovered from, and a parse that still fails lists those
    /// on its way to the error.
    pub fn parse_recovering(&self, input: &str, start: &str) -> Result<Tree, ParseError> {
        self.parse_recovering_with(input, start, Limits::default())
    }

    /// Parse in recovering mode as [`Parser::parse_recovering`] does, with the limits given.
    pub fn parse_recovering_with(
        &self,
        input: &str,
        start: &str,
        limits: Limits,
    ) -> Result<Tree, ParseError> {
        let how = Run {
            recovering: true,
            ..Run::default()
        };
        self.run(false, input, start, limits, how).0
    }

    /// Parse as [`Parser::parse_with`] does and count what each rule did: how often it was
    /// called and failed, the steps spent in it, and what remembering it answered or would
    /// answer. For finding the rules that take the time. The counting slows the parse, and
    /// the calls it keeps to tell the repeats count against the memory limit.
    ///
    /// The parse runs without the passes of the optimizer, so that every rule keeps the calls,
    /// failures and steps of the grammar as written, but for left recursion run as a loop,
    /// where the parse runs it so, which decides the calls the parse repeats. A parse with the
    /// passes takes fewer steps on most grammars, though not on all, since a test of the next
    /// byte that passes over no code adds one.
    pub fn profile(
        &self,
        input: &str,
        start: &str,
        limits: Limits,
    ) -> (Result<Tree, ParseError>, Profile) {
        let how = Run {
            profiling: true,
            ..Run::default()
        };
        let (result, profile) = self.run(true, input, start, limits, how);
        (result, profile.unwrap_or_default())
    }

    /// The names of the grammar's kinds and fields, which its trees share.
    pub fn names(&self) -> &Names {
        &self.program.names
    }

    /// The kind table, indexed by [`KindId`](crate::KindId).
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

/// The counts of two runs of a parse, or of the first where the second has none.
fn joined(first: Option<Profile>, second: Option<Profile>) -> Option<Profile> {
    match (first, second) {
        (Some(mut first), Some(second)) => {
            first.add(second);
            Some(first)
        }
        (first, _) => first,
    }
}

/// What a run of a program gave: its result, or none where it ran out of its budget, the steps
/// it took, and its counts where it was profiling.
struct Attempt {
    result: Option<Result<Tree, ParseError>>,
    steps: u64,
    profile: Option<Profile>,
}

/// Where each recovery's label failed and the floor it failed under, which a run after the
/// first asks about.
fn asked(found: &[Found]) -> Vec<(u32, Option<u32>)> {
    (found.iter())
        .map(|found| {
            let floor = found.floor.map(|floor| floor as u32);
            (found.recovery.start as u32, floor)
        })
        .collect()
}

/// The recoveries, each with what failed where its label failed under its floor, as a run
/// that collected it found.
fn explained(diagnosed: &Machine<'_, '_>, found: &[Found]) -> Vec<Recovery> {
    (found.iter())
        .map(|found| {
            let (start, floor) = (found.recovery.start as u32, found.floor.map(|f| f as u32));
            Recovery {
                expected: diagnosed.expected(start, floor),
                ..found.recovery.clone()
            }
        })
        .collect()
}

/// How a parse runs.
#[derive(Clone, Copy, Debug, Default)]
struct Run {
    /// Whether it counts what each rule did.
    profiling: bool,
    /// Whether it recovers from labeled errors.
    recovering: bool,
}

/// Parse the whole input with a program in at most `budget` steps, as `how` says.
fn attempt(
    program: &Program,
    input: &str,
    start: &str,
    limits: Limits,
    budget: u64,
    how: Run,
) -> Attempt {
    let refused = |kind| Attempt {
        result: Some(Err(ParseError {
            position: 0,
            kind,
            recoveries: Vec::new(),
        })),
        steps: 0,
        profile: None,
    };
    let Some(&entry) = program.entries.get(start) else {
        return refused(ParseErrorKind::UnknownStart(start.to_owned()));
    };
    if u32::try_from(input.len()).is_err() {
        return refused(ParseErrorKind::Limit(Limit::InputSize));
    }
    let machine = |diagnose: &[(u32, Option<u32>)]| {
        let machine = Machine::new(program, input, limits, diagnose).budgeted(budget);
        if how.recovering {
            machine.recovering()
        } else {
            machine
        }
    };
    // A run after the first collects what failed where the errors stand, and for a failed
    // parse the attempts of error alternatives. It follows the first exactly, so it ends the
    // same way.
    let diagnosed = |positions: &[(u32, Option<u32>)], explain: bool| {
        let mut again = machine(positions);
        if explain && program.explains {
            again = again.explaining();
        }
        let _ = again.run(entry);
        again
    };
    let mut first = machine(&[]);
    if how.profiling {
        first = first.profiling();
    }
    let outcome = first.run(entry);
    let profile = first.profile();
    let steps = first.steps();
    let farthest = first.farthest();
    let position = farthest.unwrap_or(0) as usize;
    let ended = first.take_recoveries();
    // The error of a failed parse as the error alternatives explain it under its floor.
    // Explaining is part of the parse, so a limit it reaches ends the parse at it.
    let explain = |again: &Machine<'_, '_>, error: ParseError, floor: Option<u32>| match again
        .explain(floor)
    {
        Ok(Some((index, start, end))) => ParseError {
            position: start as usize,
            kind: ParseErrorKind::ErrorAlternative {
                message: (program.error_messages.get(index as usize).cloned()).unwrap_or_default(),
                end: end as usize,
            },
            recoveries: error.recoveries,
        },
        Err(Stop::Limit(limit)) => ParseError {
            position,
            kind: ParseErrorKind::Limit(limit),
            recoveries: Vec::new(),
        },
        Ok(None) | Err(_) => error,
    };
    let result = match outcome {
        Ok(()) => match first.tree() {
            Ok((tree, found)) if found.is_empty() => Ok(tree),
            Ok((mut tree, found)) => {
                // The second run holds its own memory, so the first gives its back.
                drop(first);
                let again = diagnosed(&asked(&found), false);
                tree.set_recoveries(explained(&again, &found));
                Ok(tree)
            }
            Err(limit) => Err(ParseError {
                position,
                kind: ParseErrorKind::Limit(limit),
                recoveries: Vec::new(),
            }),
        },
        Err(Stop::NoMatch) => {
            drop(first);
            let mut positions = asked(&ended);
            positions.extend(farthest.map(|at| (at, None)));
            if positions.is_empty() && !program.explains {
                Err(ParseError {
                    position,
                    kind: ParseErrorKind::NoMatch {
                        expected: Vec::new(),
                    },
                    recoveries: Vec::new(),
                })
            } else {
                let again = diagnosed(&positions, true);
                let expected = farthest.map_or_else(Vec::new, |at| again.expected(at, None));
                let error = ParseError {
                    position,
                    kind: ParseErrorKind::NoMatch { expected },
                    recoveries: explained(&again, &ended),
                };
                Err(explain(&again, error, None))
            }
        }
        Err(Stop::Error {
            label,
            position,
            floor,
        }) => {
            drop(first);
            let mut positions = asked(&ended);
            // The parse got further than the label, so the farthest failure says more.
            let further = farthest.filter(|&at| floor.is_none() && at > position);
            if let Some(at) = further {
                positions.push((at, None));
                let again = diagnosed(&positions, true);
                let error = ParseError {
                    position: at as usize,
                    kind: ParseErrorKind::NoMatch {
                        expected: again.expected(at, None),
                    },
                    recoveries: explained(&again, &ended),
                };
                return Attempt {
                    result: Some(Err(explain(&again, error, None))),
                    steps,
                    profile,
                };
            }
            positions.push((position, floor));
            let again = diagnosed(&positions, true);
            let (label, named) = match program.labels.get(label as usize) {
                Some(ir::Label::Named(name)) => (name.clone(), true),
                Some(ir::Label::Message(text)) => (text.clone(), false),
                None => (String::new(), false),
            };
            let error = ParseError {
                position: position as usize,
                kind: ParseErrorKind::Label {
                    label,
                    named,
                    expected: again.expected(position, floor),
                },
                recoveries: explained(&again, &ended),
            };
            Err(explain(&again, error, floor))
        }
        Err(Stop::Limit(limit)) => Err(ParseError {
            position,
            kind: ParseErrorKind::Limit(limit),
            recoveries: Vec::new(),
        }),
        Err(Stop::Budget) => {
            return Attempt {
                result: None,
                steps,
                profile,
            };
        }
    };
    Attempt {
        result: Some(result),
        steps,
        profile,
    }
}
