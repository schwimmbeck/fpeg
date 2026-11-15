// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use crate::diag::Diagnostic;
use crate::grammar::Grammar;
use crate::ir::{self, CaptureId, FlagId, LabelId, RuleId};
use crate::lines::LineIndex;
use crate::parse::{
    END_OF_INPUT, Found, INDENTATION, Limit, Limits, ParseError, ParseErrorKind, Recovery,
};
use crate::tree::{FieldId, Kind, KindId, Names, Tree};

/// Stack reserved for the interpreter's thread. Deep rule nesting recurses deeply.
const STACK_BYTES: usize = 256 * 1024 * 1024;

/// Matches that may enclose each other before a parse stops with the depth limit, whatever the
/// rule bodies look like. A debug build on Windows takes about 7 KB of stack per match and other
/// targets differ, so this keeps the recursion within `STACK_BYTES` with room to spare. The most
/// nested Python file of CPython's tests needs about 1000.
const MAX_NESTED_EVALUATIONS: u32 = 10_000;

/// An item as the interpreter produces it, before the tree is laid out.
#[derive(Clone, Debug, PartialEq)]
enum Item {
    Token {
        kind: KindId,
        field: FieldId,
        hidden: bool,
        start: u32,
        end: u32,
    },
    Node {
        kind: KindId,
        field: FieldId,
        start: u32,
        end: u32,
        children: Vec<Item>,
    },
    /// The error item of a recovery: an `<error>` node over what it produced, or a `<missing>`
    /// token where it matched nothing.
    Error {
        label: LabelId,
        field: FieldId,
        start: u32,
        end: u32,
        /// The floor the label failed under.
        floor: Option<usize>,
        children: Vec<Item>,
    },
}

impl Item {
    fn field(&self) -> FieldId {
        match self {
            Item::Token { field, .. } | Item::Node { field, .. } | Item::Error { field, .. } => {
                *field
            }
        }
    }

    fn set_field(&mut self, value: FieldId) {
        match self {
            Item::Token { field, .. } | Item::Node { field, .. } | Item::Error { field, .. } => {
                *field = value;
            }
        }
    }

    fn is_hidden_token(&self) -> bool {
        matches!(self, Item::Token { hidden: true, .. })
    }

    /// How many items `items` holds at any depth, counted without recursion.
    fn count(items: &[Item]) -> u64 {
        let mut pending = vec![items];
        let mut count = 0;
        while let Some(items) = pending.pop() {
            count += items.len() as u64;
            for item in items {
                if let Item::Node { children, .. } | Item::Error { children, .. } = item {
                    pending.push(children);
                }
            }
        }
        count
    }

    /// The span of a node over `children`: from the first token to the last, at any depth,
    /// or empty at `at` when there is no token. An error item counts as a token, since it is
    /// one where it matched nothing and holds the tokens of its span otherwise.
    fn span_of(children: &[Item], at: u32) -> (u32, u32) {
        let mut span: Option<(u32, u32)> = None;
        for child in children {
            let (start, end, has_token) = match child {
                Item::Token { start, end, .. } | Item::Error { start, end, .. } => {
                    (*start, *end, true)
                }
                Item::Node {
                    start,
                    end,
                    children,
                    ..
                } => (*start, *end, contains_token(children)),
            };
            if has_token {
                span = Some(match span {
                    None => (start, end),
                    Some((first, _)) => (first, end),
                });
            }
        }
        span.unwrap_or((at, at))
    }
}

fn contains_token(items: &[Item]) -> bool {
    items.iter().any(|item| match item {
        Item::Token { .. } | Item::Error { .. } => true,
        Item::Node { children, .. } => contains_token(children),
    })
}

/// The error items among `items` at any depth, in the order of the text, as recoveries.
fn recoveries_in(items: &[Item], labels: &[ir::Label], out: &mut Vec<Found>) {
    for item in items {
        match item {
            Item::Token { .. } => {}
            Item::Node { children, .. } => recoveries_in(children, labels, out),
            Item::Error {
                label,
                start,
                end,
                floor,
                children,
                ..
            } => {
                out.push(Found {
                    recovery: recovery(*label, *start, *end, labels),
                    floor: *floor,
                });
                recoveries_in(children, labels, out);
            }
        }
    }
}

/// A recovery as a caller sees it, with the name of its label.
fn recovery(label: LabelId, start: u32, end: u32, labels: &[ir::Label]) -> Recovery {
    let label = match labels.get(label.0 as usize) {
        Some(ir::Label::Named(name) | ir::Label::Message(name)) => name.clone(),
        None => String::new(),
    };
    Recovery {
        label,
        start: start as usize,
        end: end as usize,
        expected: Vec::new(),
    }
}

/// Lay out items, in order, as the top level of a tree. It recurses as deeply as the tree
/// nests, which the interpreter's own stack allows, since it built the items recursively.
fn tree_from_items(items: Vec<Item>, names: Arc<Names>) -> Tree {
    let mut tree = Tree::empty(names);
    for item in items {
        push_item(&mut tree, item);
    }
    tree
}

/// Append an item and its children in postorder, and return how many entries it made.
fn push_item(tree: &mut Tree, item: Item) -> u32 {
    match item {
        // What a recovery that matched nothing produced has no text, and a token no children.
        Item::Error {
            field, start, end, ..
        } if start == end => {
            tree.push_token(KindId::MISSING, false, start, end, field);
            1
        }
        Item::Error {
            field,
            start,
            end,
            children,
            ..
        } => {
            let size = 1 + children
                .into_iter()
                .map(|c| push_item(tree, c))
                .sum::<u32>();
            tree.push_node(KindId::ERROR, start, end, size, field);
            size
        }
        Item::Token {
            kind,
            field,
            hidden,
            start,
            end,
        } => {
            tree.push_token(kind, hidden, start, end, field);
            1
        }
        Item::Node {
            kind,
            field,
            start,
            end,
            children,
        } => {
            let size = 1 + children
                .into_iter()
                .map(|c| push_item(tree, c))
                .sum::<u32>();
            tree.push_node(kind, start, end, size, field);
            size
        }
    }
}

/// A grammar resolved for the reference interpreter.
pub struct Reference {
    grammar: ir::Grammar,
}

// Not derived: the core language would print a shared item at every use, which doubles with
// each level of nested separated repetitions.
impl fmt::Debug for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reference")
            .field("rules", &self.grammar.rules.len())
            .finish_non_exhaustive()
    }
}

impl Reference {
    /// Evaluation steps a parse may take by default. Work beyond a plain step counts too, so the
    /// limit bounds the time a parse takes: copying a remembered result counts one step per
    /// item, and capturing, comparing, looking up or scanning text counts one per byte. About
    /// ten seconds, enough for a megabyte of Python.
    pub const DEFAULT_MAX_STEPS: u64 = 200_000_000;

    /// Resolve a grammar, or report the problems in it, the first hundred at most.
    pub fn new(grammar: &Grammar) -> Result<Reference, Vec<Diagnostic>> {
        crate::resolve::resolve(grammar).map(|grammar| Reference { grammar })
    }

    /// The kind table, as [`crate::Parser::kinds`] has it.
    pub fn kinds(&self) -> Vec<Kind> {
        self.grammar.kinds()
    }

    /// Whether a parse from `start` always leaves one visible item, as
    /// [`crate::Parser::single_item`] has it.
    pub fn single_item(&self, start: &str) -> Option<bool> {
        self.grammar.single_item.get(start).copied()
    }

    /// Parse the whole input from the rule `start`, with the default depth and
    /// [`Reference::DEFAULT_MAX_STEPS`] of the interpreter's own steps.
    pub fn parse(&self, input: &str, start: &str) -> Result<Tree, ParseError> {
        let limits = Limits {
            max_steps: Reference::DEFAULT_MAX_STEPS,
            ..Limits::default()
        };
        self.parse_with(input, start, limits)
    }

    /// Parse the whole input from the rule `start`.
    pub fn parse_with(&self, input: &str, start: &str, limits: Limits) -> Result<Tree, ParseError> {
        self.run(input, start, limits, true, false)
    }

    /// Parse without remembering rule results, which must give the same result as
    /// [`Reference::parse_with`] whenever neither hits a limit. For tests of the memo.
    pub fn parse_unmemoized(
        &self,
        input: &str,
        start: &str,
        limits: Limits,
    ) -> Result<Tree, ParseError> {
        self.run(input, start, limits, false, false)
    }

    /// Parse in recovering mode, where a label that `%recover` gives a recovery puts an error
    /// item in the tree where it fails, and the parse goes on.
    pub fn parse_recovering(
        &self,
        input: &str,
        start: &str,
        limits: Limits,
    ) -> Result<Tree, ParseError> {
        self.run(input, start, limits, true, true)
    }

    /// Parse in recovering mode without remembering rule results, which must give the same
    /// result as [`Reference::parse_recovering`] whenever neither hits a limit.
    pub fn parse_recovering_unmemoized(
        &self,
        input: &str,
        start: &str,
        limits: Limits,
    ) -> Result<Tree, ParseError> {
        self.run(input, start, limits, false, true)
    }

    fn run(
        &self,
        input: &str,
        start: &str,
        limits: Limits,
        memoize: bool,
        recovering: bool,
    ) -> Result<Tree, ParseError> {
        let Some(start_expr) = self.grammar.entry.get(start) else {
            return Err(ParseError {
                position: 0,
                kind: ParseErrorKind::UnknownStart(start.to_owned()),
                recoveries: Vec::new(),
            });
        };
        if u32::try_from(input.len()).is_err() {
            return Err(ParseError {
                position: 0,
                kind: ParseErrorKind::Limit(Limit::InputSize),
                recoveries: Vec::new(),
            });
        }
        let run = || {
            let interp = Interp::new(&self.grammar, input, limits, memoize);
            interp.parse(start_expr, recovering)
        };
        let outcome = std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(STACK_BYTES)
                .spawn_scoped(scope, run)
                .map(|handle| handle.join())
        });
        match outcome {
            Ok(Ok(result)) => result,
            // On the caller's stack the nesting limit would not keep the recursion in bounds,
            // so without its own thread the parse stops as if too deep.
            Err(_) => Err(ParseError {
                position: 0,
                kind: ParseErrorKind::Limit(Limit::Depth),
                recoveries: Vec::new(),
            }),
            Ok(Err(panic)) => std::panic::resume_unwind(panic),
        }
    }
}

/// The context of a match: flags that are on, the reference column, what the captures map to,
/// and, inside a recovery, its floor. Contexts that map the same compare equal, whatever was
/// captured and hidden before.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Ctx {
    flags: BTreeSet<FlagId>,
    column: u32,
    captures: Captures,
    /// Where the recovery in progress started, past which alone a label recovers.
    floor: Option<usize>,
}

/// What each capture name maps to, as the id of an interned text. Equal texts get equal ids, so
/// contexts compare, hash and copy in constant time however long the texts are.
type Captures = BTreeMap<CaptureId, u32>;

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

/// What a failure expects, shown as text only when an expected set is reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Failed {
    Kind(KindId),
    EndOfInput,
    /// A column check failed.
    Indentation,
}

/// What ends the whole parse: a labeled error, with the floor it failed under and the
/// recoveries made before it, or a limit.
#[derive(Debug)]
enum Abort {
    Error {
        label: LabelId,
        position: usize,
        floor: Option<usize>,
        recoveries: Vec<Found>,
    },
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

/// A remembered call result, and the first serial not yet given out when it was made.
struct MemoEntry {
    result: CallResult,
    made: u64,
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
    /// Results of calls that depend on no outside table entry, keyed with the quiet state and
    /// whether a lookahead encloses the call, where error alternatives record no attempts.
    memo: HashMap<(CallKey, bool, bool), MemoEntry>,
    memoize: bool,
    /// Whether the parse recovers from labeled errors.
    recovering: bool,
    /// How many tokens are being matched, at most one, since nothing inside a token makes one.
    tokens: u32,
    /// The captured texts by id, and the id of each.
    texts: Vec<String>,
    text_ids: HashMap<String, u32>,
    /// The smallest depth of a table entry the evaluation in progress consulted.
    dependency: usize,
    farthest: Option<usize>,
    /// What failed at each position, outside lookahead and the skip expression.
    expected_at: HashMap<(usize, Option<usize>), BTreeSet<Failed>>,
    quiet: u32,
    steps: u64,
    depth: u32,
    /// How many matches enclose the one in progress.
    nesting: u32,
    limits: Limits,
    /// How many lookaheads enclose the match in progress, where no error alternative makes an
    /// attempt.
    looking: u32,
    /// The attempts of error alternatives the parse made, which explain it where it fails.
    attempts: Vec<Attempt>,
}

/// An error alternative matched outside a lookahead: which, where and in which context, so that
/// it can be tested there once the parse has failed.
struct Attempt {
    index: u32,
    pos: usize,
    ctx: Ctx,
}

/// What each error alternative of a grammar tests, and its message, by index.
fn error_alternatives(g: &ir::Grammar) -> Vec<Option<(&ir::Expr, &str)>> {
    let mut table = vec![None; g.error_alternatives as usize];
    let mut seen = HashSet::new();
    let mut pending: Vec<&ir::Expr> = g.rules.iter().map(|rule| &rule.body).collect();
    pending.extend(g.skip.iter());
    pending.extend(g.recoveries.iter().flatten());
    while let Some(expr) = pending.pop() {
        match expr {
            // A shared part stands at each of its uses.
            ir::Expr::Shared(inner) if !seen.insert(Arc::as_ptr(inner)) => continue,
            ir::Expr::ErrorAlternative {
                expr: tested,
                message,
                index,
            } => {
                if let Some(slot) = table.get_mut(*index as usize) {
                    *slot = Some((&**tested, message.as_str()));
                }
            }
            _ => {}
        }
        pending.extend(expr.children());
    }
    table
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
            recovering: false,
            tokens: 0,
            texts: Vec::new(),
            text_ids: HashMap::new(),
            dependency: usize::MAX,
            farthest: None,
            expected_at: HashMap::new(),
            quiet: 0,
            steps: 0,
            nesting: 0,
            depth: 0,
            limits,
            looking: 0,
            attempts: Vec::new(),
        }
    }

    fn parse(mut self, start: &ir::Expr, recovering: bool) -> Result<Tree, ParseError> {
        self.recovering = recovering;
        let (error, floor) = match self.run(start) {
            Ok(Some((end, items))) => {
                let mut found = Vec::new();
                recoveries_in(&items, &self.g.labels, &mut found);
                if end == self.input.len() {
                    let mut tree = tree_from_items(items, Arc::clone(&self.g.names));
                    tree.set_recoveries(self.explained(&found));
                    return Ok(tree);
                }
                self.record_expected(end, Failed::EndOfInput, None);
                let mut error = self.no_match();
                error.recoveries = self.explained(&found);
                (error, None)
            }
            Ok(None) => (self.no_match(), None),
            Err(Abort::Error {
                label,
                position,
                floor,
                recoveries,
            }) => {
                let (text, named) = match self.g.labels.get(label.0 as usize) {
                    Some(ir::Label::Message(text)) => (text.clone(), false),
                    Some(ir::Label::Named(name)) => (name.clone(), true),
                    None => (String::new(), false),
                };
                let error = ParseError {
                    position,
                    kind: ParseErrorKind::Label {
                        label: text,
                        named,
                        expected: self.expected_at(position, floor),
                    },
                    recoveries: self.explained(&recoveries),
                };
                (error, floor)
            }
            Err(Abort::Limit(limit)) => {
                return Err(ParseError {
                    position: self.farthest.unwrap_or(0),
                    kind: ParseErrorKind::Limit(limit),
                    recoveries: Vec::new(),
                });
            }
        };
        Err(self.explain(error, floor))
    }

    /// Match the start expression at the start of the input, and the skip after it: the end and
    /// the items, or `None` where the start expression fails.
    fn run(&mut self, start: &ir::Expr) -> Result<Option<(usize, Vec<Item>)>, Abort> {
        let ctx = Ctx {
            flags: BTreeSet::new(),
            column: 0,
            captures: Captures::new(),
            floor: None,
        };
        self.eval(start, 0, &ctx).and_then(|outcome| match outcome {
            // An error of the skip after the start rule comes after its items.
            Outcome::Ok(success) => match self.skip_end(success.end, &ctx) {
                Ok(end) => Ok(Some((end, success.items))),
                Err(abort) => Err(self.after(&success.items, abort)),
            },
            Outcome::Fail { .. } => Ok(None),
        })
    }

    /// The error of a failed parse as the grammar's error alternatives explain it. The attempts
    /// under the floor of the error are tested in the order of their positions, and at one
    /// position in the order of the alternatives, each matched where it was made as a parse of
    /// its own. The first alternative that matches, with its longest match, gives its message
    /// over what it matched in place of the error. Explaining is part of the parse: the tests
    /// have the steps the parse left, and a limit they reach ends the parse at it.
    fn explain(&self, error: ParseError, floor: Option<usize>) -> ParseError {
        let table = error_alternatives(self.g);
        let mut attempts: Vec<&Attempt> = (self.attempts.iter())
            .filter(|attempt| attempt.ctx.floor == floor)
            .collect();
        attempts.sort_by_key(|attempt| (attempt.pos, attempt.index));
        let left = Limits {
            max_steps: self.limits.max_steps.saturating_sub(self.steps),
            ..self.limits
        };
        let mut tester = Interp::new(self.g, self.input, left, self.memoize);
        // A context names its captures by the ids of the parse's texts.
        tester.texts = self.texts.clone();
        tester.text_ids = self.text_ids.clone();
        // A test is sheltered: it records no failure, and a label in it fails it.
        tester.quiet = 1;
        tester.looking = 1;
        let mut seen = HashSet::new();
        let mut found: Option<(&Attempt, usize)> = None;
        for attempt in attempts {
            let Some(&Some((tested, _))) = table.get(attempt.index as usize) else {
                continue;
            };
            if found
                .is_some_and(|(first, _)| (first.pos, first.index) != (attempt.pos, attempt.index))
            {
                break;
            }
            let ctx = Ctx {
                floor: None,
                ..attempt.ctx.clone()
            };
            if !seen.insert((attempt.index, attempt.pos, ctx.clone())) {
                continue;
            }
            match tester.eval(tested, attempt.pos, &ctx) {
                Ok(Outcome::Ok(success)) if found.is_none_or(|(_, end)| success.end > end) => {
                    found = Some((attempt, success.end));
                }
                Err(Abort::Limit(limit)) => {
                    return ParseError {
                        position: self.farthest.unwrap_or(0),
                        kind: ParseErrorKind::Limit(limit),
                        recoveries: Vec::new(),
                    };
                }
                _ => {}
            }
        }
        let message = found.and_then(|(attempt, _)| table.get(attempt.index as usize)?.as_ref());
        match (found, message) {
            (Some((attempt, end)), Some((_, message))) => ParseError {
                position: attempt.pos,
                kind: ParseErrorKind::ErrorAlternative {
                    message: (*message).to_owned(),
                    end,
                },
                recoveries: error.recoveries,
            },
            _ => error,
        }
    }

    fn no_match(&mut self) -> ParseError {
        let position = self.farthest.unwrap_or(0);
        ParseError {
            position,
            kind: ParseErrorKind::NoMatch {
                expected: self.expected_at(position, None),
            },
            recoveries: Vec::new(),
        }
    }

    /// An error of a part, which ends the whole, after the parts before it produced `items`:
    /// the recoveries among those come first.
    fn after(&self, items: &[Item], abort: Abort) -> Abort {
        match abort {
            Abort::Error {
                label,
                position,
                floor,
                recoveries,
            } => {
                let mut before = Vec::new();
                recoveries_in(items, &self.g.labels, &mut before);
                before.extend(recoveries);
                Abort::Error {
                    label,
                    position,
                    floor,
                    recoveries: before,
                }
            }
            limit => limit,
        }
    }

    /// What failed at a position under a floor over the whole parse, as an expected set shows
    /// it, sorted.
    fn expected_at(&self, position: usize, floor: Option<usize>) -> Vec<String> {
        let Some(failed) = self.expected_at.get(&(position, floor)) else {
            return Vec::new();
        };
        let shown: BTreeSet<String> = (failed.iter())
            .map(|what| match what {
                Failed::Kind(kind) => self.kind_display(*kind),
                Failed::EndOfInput => END_OF_INPUT.to_owned(),
                Failed::Indentation => INDENTATION.to_owned(),
            })
            .collect();
        shown.into_iter().collect()
    }

    /// The recoveries, each with what failed where its label failed under its floor.
    fn explained(&self, found: &[Found]) -> Vec<Recovery> {
        (found.iter())
            .map(|found| Recovery {
                expected: self.expected_at(found.recovery.start, found.floor),
                ..found.recovery.clone()
            })
            .collect()
    }

    /// Record a failure at a position: what fails at the farthest one is the expected set of a
    /// parse that does not match, and what fails where a label fails explains the error.
    fn record_expected(&mut self, position: usize, what: Failed, floor: Option<usize>) {
        if self.quiet > 0 {
            return;
        }
        if floor.is_none() && self.farthest.is_none_or(|farthest| position > farthest) {
            self.farthest = Some(position);
        }
        let failed = self.expected_at.entry((position, floor)).or_default();
        failed.insert(what);
    }

    fn char_at(&self, pos: usize) -> Option<char> {
        self.input.get(pos..).and_then(|rest| rest.chars().next())
    }

    /// The column of `pos`, counting the scan from the start of its line as steps.
    fn column(&mut self, pos: usize) -> Result<u32, Abort> {
        self.spend((pos - self.lines.line_start(pos)) as u64)?;
        Ok(self.lines.column(self.input, pos, self.g.tab_width))
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
            floor: ctx.floor,
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
            let result = entry.result.clone();
            self.charge(&result)?;
            return Ok(Interp::call_outcome(result, ctx));
        }
        let memo_key = (key.clone(), self.quiet > 0, self.looking > 0);
        // The result consulted none of the entries active when it was made. Matching it again
        // would follow the same path unless it consulted an entry made since, and it can only
        // reach entries at its own position. Those are the top of the stack, since a call
        // starts no earlier than its caller, and the ones below the topmost are older than it.
        // So the result holds unless the topmost entry at its position is newer than it.
        if let Some(entry) = self.memo.get(&memo_key)
            && (self.lr_active.last()).is_none_or(|&(serial, at)| at < pos || serial < entry.made)
        {
            let result = entry.result.clone();
            self.charge(&result)?;
            return Ok(Interp::call_outcome(result, ctx));
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
            self.charge(&best)?;
            let entry = MemoEntry {
                result: best.clone(),
                made: self.next_serial,
            };
            self.memo.insert(memo_key, entry);
        }
        Ok(Interp::call_outcome(best, ctx))
    }

    /// Count work beyond a plain step as steps, so that the step limit bounds the time it takes.
    fn spend(&mut self, steps: u64) -> Result<(), Abort> {
        self.steps += steps;
        if self.steps > self.limits.max_steps {
            return Err(Abort::Limit(Limit::Steps));
        }
        Ok(())
    }

    /// Count a copy of a result as one step per item copied.
    fn charge(&mut self, result: &CallResult) -> Result<(), Abort> {
        self.spend(result.as_ref().map_or(0, |(_, items)| Item::count(items)))
    }

    /// The id of a captured text, the same for equal texts.
    fn intern(&mut self, text: &str) -> u32 {
        if let Some(&id) = self.text_ids.get(text) {
            return id;
        }
        // Each new text costs its length in steps first, so ids stay far below u32::MAX.
        let id = self.texts.len() as u32;
        self.texts.push(text.to_owned());
        self.text_ids.insert(text.to_owned(), id);
        id
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
        while self.lr.get(key).is_some_and(|entry| entry.used) {
            let seed = Some(best.clone());
            self.charge(&seed)?;
            if let Some(entry) = self.lr.get_mut(key) {
                entry.result = seed;
                entry.used = false;
            }
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
                    let outcome = match self.eval(part, end, &part_ctx) {
                        Ok(outcome) => outcome,
                        Err(abort) => return Err(self.after(&items, abort)),
                    };
                    match outcome {
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
                    let outcome = match self.eval(expr, end, &step_ctx) {
                        Ok(outcome) => outcome,
                        Err(abort) => return Err(self.after(&items, abort)),
                    };
                    match outcome {
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
            E::Token { kind, hidden, expr } => {
                self.tokens += 1;
                let outcome = self.eval(expr, pos, ctx);
                self.tokens -= 1;
                match outcome? {
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
                        self.record_expected(pos, Failed::Kind(*kind), ctx.floor);
                        Ok(Outcome::Fail { cut })
                    }
                }
            }
            E::Node { kind, expr } => self.node(*kind, expr, pos, ctx, false),
            E::Transparent { kind, expr } => self.node(*kind, expr, pos, ctx, true),
            E::Field { field, expr } => match self.eval(expr, pos, ctx)? {
                Outcome::Ok(mut success) => {
                    for item in &mut success.items {
                        if item.field() == FieldId::NONE && !item.is_hidden_token() {
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
                    // Looking a match up is work of its length.
                    self.spend((success.end - pos) as u64)?;
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
                let column = self.column(at)?;
                if column > ctx.column {
                    let inner = Ctx {
                        column,
                        ..ctx.clone()
                    };
                    self.eval(expr, pos, &inner)
                } else {
                    self.record_expected(at, Failed::Indentation, ctx.floor);
                    Interp::fail()
                }
            }
            E::Same { skip } => {
                let at = if *skip { self.skip_end(pos, ctx)? } else { pos };
                if self.column(at)? == ctx.column {
                    Interp::ok(pos, ctx)
                } else {
                    self.record_expected(at, Failed::Indentation, ctx.floor);
                    Interp::fail()
                }
            }
            E::Def {
                capture,
                expr,
                skip,
            } => {
                let start = if *skip { self.skip_end(pos, ctx)? } else { pos };
                match self.eval(expr, pos, ctx)? {
                    Outcome::Ok(mut success) => {
                        let start = start.min(success.end);
                        let text = self.input.get(start..success.end).unwrap_or_default();
                        self.spend(text.len() as u64)?;
                        let id = self.intern(text);
                        success.captures.insert(*capture, id);
                        Ok(Outcome::Ok(success))
                    }
                    failure => Ok(failure),
                }
            }
            E::Ref { capture } => {
                let Some(text) = ctx
                    .captures
                    .get(capture)
                    .map(|&id| &self.texts[id as usize])
                else {
                    return Interp::fail();
                };
                let length = text.len();
                let matches = (self.input.get(pos..)).is_some_and(|r| r.starts_with(text.as_str()));
                // Comparing a long text is work of its length.
                self.spend(length as u64)?;
                if matches {
                    Interp::ok(pos + length, ctx)
                } else {
                    Interp::fail()
                }
            }
            E::Label { label, expr, skip } => match self.eval(expr, pos, ctx)? {
                // Any failure, also one after a cut, since a label is no choice point.
                Outcome::Fail { cut } => {
                    let position = if *skip { self.skip_end(pos, ctx)? } else { pos };
                    let field = match &**expr {
                        E::Field { field, .. } => *field,
                        _ => FieldId::NONE,
                    };
                    self.recover(*label, field, position, cut, ctx)
                }
                success => Ok(success),
            },
            E::ErrorAlternative { index, .. } => {
                if self.looking == 0 && self.g.error_alternatives > 0 {
                    self.attempts.push(Attempt {
                        index: *index,
                        pos,
                        ctx: ctx.clone(),
                    });
                }
                Interp::fail()
            }
            E::Shared(expr) => self.eval(expr, pos, ctx),
        }
    }

    /// What a label that failed at `position` gives: the error, or in recovering mode, past the
    /// floor, the error item of the label's recovery where it succeeds. A recovery runs in
    /// recovering mode with its start as the floor, so one nested in it starts further, and
    /// keeps the cut flag of the failure, since a cut that the label's expression passed still
    /// commits the choice around it.
    fn recover(
        &mut self,
        label: LabelId,
        field: FieldId,
        position: usize,
        cut: bool,
        ctx: &Ctx,
    ) -> Eval {
        let error = Abort::Error {
            label,
            position,
            floor: ctx.floor,
            recoveries: Vec::new(),
        };
        // Inside a lookahead, a token and the skip expression what is made is dropped, so a
        // recovery there would go unreported.
        let sheltered = self.quiet > 0 || self.tokens > 0;
        let past_floor = ctx.floor.is_none_or(|floor| position > floor);
        let recovery = (self.g.recoveries.get(label.0 as usize)).and_then(Option::as_ref);
        let Some(recovery) = recovery.filter(|_| self.recovering && !sheltered && past_floor)
        else {
            return Err(error);
        };
        let inner = Ctx {
            floor: Some(position),
            ..ctx.clone()
        };
        match self.eval(recovery, position, &inner)? {
            Outcome::Ok(success) => Ok(Outcome::Ok(Success {
                end: success.end,
                items: vec![Item::Error {
                    label,
                    field,
                    start: position as u32,
                    end: success.end as u32,
                    floor: ctx.floor,
                    children: success.items,
                }],
                captures: ctx.captures.clone(),
                cut,
            })),
            Outcome::Fail { .. } => Err(error),
        }
    }

    /// Whether `expr` succeeds at `pos`, without consuming input, recording failures or
    /// keeping captures. A labeled error counts as a failure there, and nothing recovers.
    fn lookahead(&mut self, expr: &ir::Expr, pos: usize, ctx: &Ctx) -> Result<bool, Abort> {
        self.quiet += 1;
        self.looking += 1;
        let outcome = self.eval(expr, pos, ctx);
        self.looking -= 1;
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
