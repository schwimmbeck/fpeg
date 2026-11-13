// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! The reference interpreter: a direct implementation of the semantics page, slow on purpose.
//!
//! It runs the core language as the semantics define it, one case per core expression, and is
//! the oracle every engine and optimization is compared against. It is compiled only for tests
//! and fuzzing, behind the `reference` feature.
//!
//! One shortcut keeps deep grammars tractable without changing a result: a rule call whose
//! evaluation consulted no left-recursion table entry from outside itself depends only on the
//! rule, the position, the context, whether failures are being recorded, and on the entries
//! made since, so its result is remembered under the first four and reused while no entry made
//! since could be reached. Without it, every operator level of a precedence block re-parses the
//! levels below it in each growing step, which is exponential in the number of levels.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use crate::grammar::{Grammar, GrammarError};
use crate::ir::{self, CaptureId, FlagId, LabelId, RuleId};
use crate::lines::LineIndex;
use crate::tree::{FieldId, Item, KindId, Tree};

/// Rule activations that may be nested before a parse stops.
pub const DEFAULT_MAX_DEPTH: u32 = 2_000;

/// Evaluation steps a parse may take before it stops.
pub const DEFAULT_MAX_STEPS: u64 = 50_000_000;

/// Stack reserved for the interpreter's thread. Deep rule nesting recurses deeply.
const STACK_BYTES: usize = 256 * 1024 * 1024;

/// Matches that may enclose each other before a parse stops with the depth limit, whatever the
/// rule bodies look like. A debug build takes about 7 KB of stack per match, so this keeps the
/// recursion well within `STACK_BYTES`.
const MAX_NESTED_EVALUATIONS: u32 = 20_000;

/// What a failed column check expects, in an expected set.
const INDENTATION: &str = "indentation";

/// What a parse that stopped before the end of the input expects, in an expected set.
const END_OF_INPUT: &str = "end of input";

/// How far a parse may go before it stops with [`ParseErrorKind::Limit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The deepest nesting of rule activations.
    pub max_depth: u32,
    /// The number of evaluation steps.
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
    /// Too deeply nested rule activations, or matches nested deeper than the interpreter's
    /// stack allows.
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

/// A grammar resolved for the reference interpreter.
#[derive(Debug)]
pub struct Reference {
    grammar: ir::Grammar,
}

impl Reference {
    /// Resolve a grammar, or report every problem in it.
    pub fn new(grammar: &Grammar) -> Result<Reference, Vec<GrammarError>> {
        crate::grammar::resolve::resolve(grammar).map(|grammar| Reference { grammar })
    }

    /// Parse the whole input from the rule `start`, with the default limits.
    pub fn parse(&self, input: &str, start: &str) -> Result<Tree, ParseError> {
        self.parse_with(input, start, Limits::default())
    }

    /// Parse the whole input from the rule `start`.
    pub fn parse_with(&self, input: &str, start: &str, limits: Limits) -> Result<Tree, ParseError> {
        self.run(input, start, limits, true)
    }

    /// Parse without remembering rule results, which must give the same result as
    /// [`Reference::parse_with`] whenever neither hits a limit. For tests of the memo.
    pub fn parse_unmemoized(
        &self,
        input: &str,
        start: &str,
        limits: Limits,
    ) -> Result<Tree, ParseError> {
        self.run(input, start, limits, false)
    }

    fn run(
        &self,
        input: &str,
        start: &str,
        limits: Limits,
        memoize: bool,
    ) -> Result<Tree, ParseError> {
        let Some(start_expr) = self.grammar.entry.get(start) else {
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
        let run = || Interp::new(&self.grammar, input, limits, memoize).parse(start_expr);
        let outcome = std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(STACK_BYTES)
                .spawn_scoped(scope, run)
                .map(|handle| handle.join())
        });
        match outcome {
            Ok(Ok(result)) => result,
            // The thread could not start, so run here and let a deep parse hit its limit.
            Err(_) => run(),
            Ok(Err(panic)) => std::panic::resume_unwind(panic),
        }
    }
}

/// The context of a match: flags that are on, the reference column, and what the captures map
/// to. Contexts that map the same compare equal, whatever was captured and hidden before.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Ctx {
    flags: BTreeSet<FlagId>,
    column: u32,
    captures: Captures,
}

type Captures = BTreeMap<CaptureId, String>;

#[derive(Debug)]
struct Success {
    end: usize,
    items: Vec<Item>,
    captures: Captures,
    cut: bool,
}

#[derive(Debug)]
enum Outcome {
    Ok(Success),
    Fail { cut: bool },
}

/// What ends the whole parse: a labeled error or a limit.
#[derive(Debug)]
enum Abort {
    Error { label: LabelId, position: usize },
    Limit(Limit),
}

type Eval = Result<Outcome, Abort>;

/// A rule, a position and a context: what a rule call's result depends on.
type CallKey = (RuleId, usize, Ctx);

/// The result of a rule call: the end and the items, or failure.
type CallResult = Option<(usize, Vec<Item>)>;

struct LrEntry {
    result: CallResult,
    used: bool,
    /// How many entries were active when this one was made. Entries form a stack.
    depth: usize,
}

/// A remembered call result, and the serials of the table entries at its position that were
/// active when it was made.
struct MemoEntry {
    result: CallResult,
    active: Vec<u64>,
}

struct Interp<'a> {
    g: &'a ir::Grammar,
    input: &'a str,
    lines: LineIndex,
    lr: HashMap<CallKey, LrEntry>,
    /// The serials and positions of the active table entries, oldest first. Every entry gets a
    /// new serial.
    lr_active: Vec<(u64, usize)>,
    next_serial: u64,
    /// Results of calls that depend on no outside table entry, keyed with the quiet state.
    memo: HashMap<(CallKey, bool), MemoEntry>,
    memoize: bool,
    /// The smallest depth of a table entry the evaluation in progress consulted.
    dependency: usize,
    farthest: Option<usize>,
    expected: BTreeSet<String>,
    quiet: u32,
    steps: u64,
    depth: u32,
    /// How many matches enclose the one in progress.
    nesting: u32,
    limits: Limits,
}

impl<'a> Interp<'a> {
    fn new(g: &'a ir::Grammar, input: &'a str, limits: Limits, memoize: bool) -> Interp<'a> {
        Interp {
            g,
            input,
            lines: LineIndex::new(input),
            lr: HashMap::new(),
            lr_active: Vec::new(),
            next_serial: 0,
            memo: HashMap::new(),
            memoize,
            dependency: usize::MAX,
            farthest: None,
            expected: BTreeSet::new(),
            quiet: 0,
            steps: 0,
            nesting: 0,
            depth: 0,
            limits,
        }
    }

    fn parse(mut self, start: &ir::Expr) -> Result<Tree, ParseError> {
        let ctx = Ctx {
            flags: BTreeSet::new(),
            column: 0,
            captures: Captures::new(),
        };
        let result = self.eval(start, 0, &ctx).and_then(|outcome| match outcome {
            Outcome::Ok(success) => {
                let end = self.skip_end(success.end, &ctx)?;
                Ok(Some((end, success.items)))
            }
            Outcome::Fail { .. } => Ok(None),
        });
        match result {
            Ok(Some((end, items))) if end == self.input.len() => {
                Ok(Tree::from_items(items, self.g.names.clone()))
            }
            Ok(Some((end, _))) => {
                self.record_expected(end, END_OF_INPUT.to_owned());
                Err(self.no_match())
            }
            Ok(None) => Err(self.no_match()),
            Err(Abort::Error { label, position }) => {
                let text = match self.g.labels.get(label.0 as usize) {
                    Some(ir::Label::Message(m) | ir::Label::Named(m)) => m.clone(),
                    None => String::new(),
                };
                Err(ParseError {
                    position,
                    kind: ParseErrorKind::Label(text),
                })
            }
            Err(Abort::Limit(limit)) => Err(ParseError {
                position: self.farthest.unwrap_or(0),
                kind: ParseErrorKind::Limit(limit),
            }),
        }
    }

    fn no_match(&mut self) -> ParseError {
        ParseError {
            position: self.farthest.unwrap_or(0),
            kind: ParseErrorKind::NoMatch {
                expected: std::mem::take(&mut self.expected).into_iter().collect(),
            },
        }
    }

    fn record_expected(&mut self, position: usize, what: String) {
        if self.quiet > 0 {
            return;
        }
        match self.farthest {
            Some(farthest) if position < farthest => {}
            Some(farthest) if position == farthest => {
                self.expected.insert(what);
            }
            _ => {
                self.farthest = Some(position);
                self.expected = BTreeSet::from([what]);
            }
        }
    }

    fn char_at(&self, pos: usize) -> Option<char> {
        self.input.get(pos..).and_then(|rest| rest.chars().next())
    }

    fn column(&self, pos: usize) -> u32 {
        self.lines.column(self.input, pos, self.g.tab_width)
    }

    fn ok(pos: usize, ctx: &Ctx) -> Eval {
        Ok(Outcome::Ok(Success {
            end: pos,
            items: Vec::new(),
            captures: ctx.captures.clone(),
            cut: false,
        }))
    }

    fn fail() -> Eval {
        Ok(Outcome::Fail { cut: false })
    }

    fn with_captures(ctx: &Ctx, captures: Captures) -> Ctx {
        Ctx {
            flags: ctx.flags.clone(),
            column: ctx.column,
            captures,
        }
    }

    fn skip_end(&mut self, pos: usize, ctx: &Ctx) -> Result<usize, Abort> {
        let Some(skip) = &self.g.skip else {
            return Ok(pos);
        };
        let mut pos = pos;
        loop {
            self.quiet += 1;
            let outcome = self.eval(skip, pos, ctx);
            self.quiet -= 1;
            match outcome? {
                Outcome::Ok(success) if success.end > pos => pos = success.end,
                _ => return Ok(pos),
            }
        }
    }

    fn call(&mut self, rule: RuleId, pos: usize, ctx: &Ctx) -> Eval {
        let key = (rule, pos, ctx.clone());
        if let Some(entry) = self.lr.get_mut(&key) {
            entry.used = true;
            self.dependency = self.dependency.min(entry.depth);
            return Ok(Interp::call_outcome(entry.result.clone(), ctx));
        }
        let memo_key = (key.clone(), self.quiet > 0);
        // The result consulted none of the entries active when it was made. Matching it again
        // would follow the same path unless it consulted an entry made since, and it can only
        // reach entries at its own position. Those are the top of the stack, since a call
        // starts no earlier than its caller.
        if let Some(entry) = self.memo.get(&memo_key)
            && self
                .lr_active
                .iter()
                .rev()
                .take_while(|&&(_, at)| at == pos)
                .all(|(serial, _)| entry.active.contains(serial))
        {
            return Ok(Interp::call_outcome(entry.result.clone(), ctx));
        }
        if self.depth >= self.limits.max_depth {
            return Err(Abort::Limit(Limit::Depth));
        }
        let Some(body) = self.g.rules.get(rule.0 as usize).map(|r| &r.body) else {
            return Interp::fail();
        };
        let height = self.lr.len();
        self.lr.insert(
            key.clone(),
            LrEntry {
                result: None,
                used: false,
                depth: height,
            },
        );
        self.lr_active.push((self.next_serial, pos));
        self.next_serial += 1;
        let outer_dependency = std::mem::replace(&mut self.dependency, usize::MAX);
        self.depth += 1;
        let result = self.grow(&key, body, pos, ctx);
        self.depth -= 1;
        self.lr.remove(&key);
        self.lr_active.pop();
        let inner_dependency = self.dependency;
        self.dependency = outer_dependency.min(inner_dependency);
        let best = result?;
        if self.memoize && inner_dependency >= height {
            let active = self
                .lr_active
                .iter()
                .rev()
                .take_while(|&&(_, at)| at == pos);
            let entry = MemoEntry {
                result: best.clone(),
                active: active.map(|&(serial, _)| serial).collect(),
            };
            self.memo.insert(memo_key, entry);
        }
        Ok(Interp::call_outcome(best, ctx))
    }

    fn call_outcome(result: CallResult, ctx: &Ctx) -> Outcome {
        match result {
            None => Outcome::Fail { cut: false },
            Some((end, items)) => Outcome::Ok(Success {
                end,
                items,
                captures: ctx.captures.clone(),
                cut: false,
            }),
        }
    }

    /// Match a rule body, then match it again with the table entry holding the last result for
    /// as long as each match ends further. A first match that never consulted the entry is not
    /// left recursive here, and matching again would give the same result.
    fn grow(
        &mut self,
        key: &CallKey,
        body: &ir::Expr,
        pos: usize,
        ctx: &Ctx,
    ) -> Result<CallResult, Abort> {
        let Outcome::Ok(first) = self.eval(body, pos, ctx)? else {
            return Ok(None);
        };
        let mut best = (first.end, first.items);
        while let Some(entry) = self.lr.get_mut(key) {
            if !entry.used {
                break;
            }
            entry.result = Some(best.clone());
            entry.used = false;
            match self.eval(body, pos, ctx)? {
                Outcome::Ok(next) if next.end > best.0 => best = (next.end, next.items),
                _ => break,
            }
        }
        Ok(Some(best))
    }

    /// Match an expression, counting how deeply matches nest, since each one recurses.
    fn eval(&mut self, expr: &ir::Expr, pos: usize, ctx: &Ctx) -> Eval {
        if self.nesting >= MAX_NESTED_EVALUATIONS {
            return Err(Abort::Limit(Limit::Depth));
        }
        self.nesting += 1;
        let result = self.step(expr, pos, ctx);
        self.nesting -= 1;
        result
    }

    fn step(&mut self, expr: &ir::Expr, pos: usize, ctx: &Ctx) -> Eval {
        use ir::Expr as E;
        self.steps += 1;
        if self.steps > self.limits.max_steps {
            return Err(Abort::Limit(Limit::Steps));
        }
        match expr {
            E::Empty => Interp::ok(pos, ctx),
            E::Literal {
                text,
                case_insensitive,
            } => {
                let rest = self.input.as_bytes().get(pos..).unwrap_or_default();
                let wanted = text.as_bytes();
                let matches = rest.len() >= wanted.len()
                    && if *case_insensitive {
                        rest[..wanted.len()].eq_ignore_ascii_case(wanted)
                    } else {
                        &rest[..wanted.len()] == wanted
                    };
                if matches {
                    Interp::ok(pos + wanted.len(), ctx)
                } else {
                    Interp::fail()
                }
            }
            E::Class(set) => match self.char_at(pos) {
                Some(c) if set.contains(c) => Interp::ok(pos + c.len_utf8(), ctx),
                _ => Interp::fail(),
            },
            E::Any => match self.char_at(pos) {
                Some(c) => Interp::ok(pos + c.len_utf8(), ctx),
                None => Interp::fail(),
            },
            E::Call(rule) => self.call(*rule, pos, ctx),
            E::Seq(parts) => {
                let mut end = pos;
                let mut items = Vec::new();
                let mut captures = ctx.captures.clone();
                let mut cut = false;
                for part in parts {
                    let part_ctx = Interp::with_captures(ctx, captures);
                    match self.eval(part, end, &part_ctx)? {
                        Outcome::Ok(success) => {
                            end = success.end;
                            items.extend(success.items);
                            captures = success.captures;
                            cut |= success.cut;
                        }
                        Outcome::Fail { cut: failed_cut } => {
                            return Ok(Outcome::Fail {
                                cut: cut || failed_cut,
                            });
                        }
                    }
                }
                Ok(Outcome::Ok(Success {
                    end,
                    items,
                    captures,
                    cut,
                }))
            }
            E::Choice(alternatives) => {
                for alternative in alternatives {
                    match self.eval(alternative, pos, ctx)? {
                        Outcome::Ok(success) => {
                            return Ok(Outcome::Ok(Success {
                                cut: false,
                                ..success
                            }));
                        }
                        Outcome::Fail { cut: true } => return Interp::fail(),
                        Outcome::Fail { cut: false } => {}
                    }
                }
                Interp::fail()
            }
            E::Repeat { expr, min, max } => {
                let mut count = 0;
                let mut end = pos;
                let mut items = Vec::new();
                let mut captures = ctx.captures.clone();
                while max.is_none_or(|max| count < max) {
                    let step_ctx = Interp::with_captures(ctx, captures.clone());
                    match self.eval(expr, end, &step_ctx)? {
                        Outcome::Ok(success) => {
                            let consumed = success.end > end;
                            count += 1;
                            end = success.end;
                            items.extend(success.items);
                            captures = success.captures;
                            if !consumed {
                                // Further matches would match the same way, so this one is
                                // the last and stands for the remaining required ones.
                                count = count.max(*min);
                                break;
                            }
                        }
                        Outcome::Fail { cut: true } => return Interp::fail(),
                        Outcome::Fail { cut: false } => break,
                    }
                }
                if count < *min {
                    return Interp::fail();
                }
                Ok(Outcome::Ok(Success {
                    end,
                    items,
                    captures,
                    cut: false,
                }))
            }
            E::And(inner) => match self.lookahead(inner, pos, ctx)? {
                true => Interp::ok(pos, ctx),
                false => Interp::fail(),
            },
            E::Not(inner) => match self.lookahead(inner, pos, ctx)? {
                true => Interp::fail(),
                false => Interp::ok(pos, ctx),
            },
            E::Cut => Ok(Outcome::Ok(Success {
                end: pos,
                items: Vec::new(),
                captures: ctx.captures.clone(),
                cut: true,
            })),
            E::Skip => {
                let end = self.skip_end(pos, ctx)?;
                Interp::ok(end, ctx)
            }
            E::Token { kind, hidden, expr } => match self.eval(expr, pos, ctx)? {
                Outcome::Ok(success) => Ok(Outcome::Ok(Success {
                    items: vec![Item::Token {
                        kind: *kind,
                        field: FieldId::NONE,
                        hidden: *hidden,
                        start: pos as u32,
                        end: success.end as u32,
                    }],
                    ..success
                })),
                Outcome::Fail { cut } => {
                    let what = self.kind_display(*kind);
                    self.record_expected(pos, what);
                    Ok(Outcome::Fail { cut })
                }
            },
            E::Node { kind, expr } => self.node(*kind, expr, pos, ctx, false),
            E::Transparent { kind, expr } => self.node(*kind, expr, pos, ctx, true),
            E::Field { field, expr } => match self.eval(expr, pos, ctx)? {
                Outcome::Ok(mut success) => {
                    for item in &mut success.items {
                        if item.field() == FieldId::NONE {
                            item.set_field(*field);
                        }
                    }
                    Ok(Outcome::Ok(success))
                }
                failure => Ok(failure),
            },
            E::WordEnd => match self.char_at(pos) {
                Some(c) if self.g.word.contains(c) => Interp::fail(),
                _ => Interp::ok(pos, ctx),
            },
            E::Unreserved(inner) => match self.eval(inner, pos, ctx)? {
                Outcome::Ok(success) => {
                    let text = self.input.get(pos..success.end).unwrap_or_default();
                    if self.g.reserved.contains(text) {
                        Interp::fail()
                    } else {
                        Ok(Outcome::Ok(success))
                    }
                }
                failure => Ok(failure),
            },
            E::FlagOn { flag, expr } => {
                let mut inner = ctx.clone();
                inner.flags.insert(*flag);
                self.eval(expr, pos, &inner)
            }
            E::FlagOff { flag, expr } => {
                let mut inner = ctx.clone();
                inner.flags.remove(flag);
                self.eval(expr, pos, &inner)
            }
            E::FlagTest { flag, negated } => {
                if ctx.flags.contains(flag) != *negated {
                    Interp::ok(pos, ctx)
                } else {
                    Interp::fail()
                }
            }
            E::Indent { expr, skip } => {
                let at = if *skip { self.skip_end(pos, ctx)? } else { pos };
                let column = self.column(at);
                if column > ctx.column {
                    let inner = Ctx {
                        column,
                        ..ctx.clone()
                    };
                    self.eval(expr, pos, &inner)
                } else {
                    self.record_expected(at, INDENTATION.to_owned());
                    Interp::fail()
                }
            }
            E::Same { skip } => {
                let at = if *skip { self.skip_end(pos, ctx)? } else { pos };
                if self.column(at) == ctx.column {
                    Interp::ok(pos, ctx)
                } else {
                    self.record_expected(at, INDENTATION.to_owned());
                    Interp::fail()
                }
            }
            E::Def { capture, expr } => match self.eval(expr, pos, ctx)? {
                Outcome::Ok(mut success) => {
                    let text = self.input.get(pos..success.end).unwrap_or_default();
                    success.captures.insert(*capture, text.to_owned());
                    Ok(Outcome::Ok(success))
                }
                failure => Ok(failure),
            },
            E::Ref { capture } => match ctx.captures.get(capture) {
                Some(text)
                    if self
                        .input
                        .get(pos..)
                        .is_some_and(|r| r.starts_with(text.as_str())) =>
                {
                    Interp::ok(pos + text.len(), ctx)
                }
                _ => Interp::fail(),
            },
            E::Throw(label) => Err(Abort::Error {
                label: *label,
                position: pos,
            }),
            E::ErrorAlternative => Interp::fail(),
        }
    }

    /// Whether `expr` succeeds at `pos`, without consuming input, recording failures or
    /// keeping captures. A labeled error counts as a failure there.
    fn lookahead(&mut self, expr: &ir::Expr, pos: usize, ctx: &Ctx) -> Result<bool, Abort> {
        self.quiet += 1;
        let outcome = self.eval(expr, pos, ctx);
        self.quiet -= 1;
        match outcome {
            Ok(Outcome::Ok(_)) => Ok(true),
            Ok(Outcome::Fail { .. }) | Err(Abort::Error { .. }) => Ok(false),
            Err(limit) => Err(limit),
        }
    }

    fn node(
        &mut self,
        kind: KindId,
        expr: &ir::Expr,
        pos: usize,
        ctx: &Ctx,
        transparent: bool,
    ) -> Eval {
        match self.eval(expr, pos, ctx)? {
            Outcome::Ok(mut success) => {
                let visible = success
                    .items
                    .iter()
                    .filter(|i| !i.is_hidden_token())
                    .count();
                if !(transparent && visible == 1) {
                    let children = std::mem::take(&mut success.items);
                    let (start, end) = Item::span_of(&children, pos as u32);
                    success.items = vec![Item::Node {
                        kind,
                        field: FieldId::NONE,
                        start,
                        end,
                        children,
                    }];
                }
                Ok(Outcome::Ok(success))
            }
            failure => Ok(failure),
        }
    }

    fn kind_display(&self, kind: KindId) -> String {
        self.g
            .kind_display
            .get(kind.0 as usize)
            .cloned()
            .unwrap_or_default()
    }
}
