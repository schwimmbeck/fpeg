// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The parsing machine: runs a program over an input and lays out the tree it built.
//!
//! The machine keeps one stack of entries on the heap, so a deep grammar never touches the
//! caller's stack. Items of nodes being built wait on a stack of their own, which a failure
//! truncates. A closed node moves its items to the arena, which a failure leaves alone: what it
//! no longer reaches is garbage until the parse ends, bounded by the step limit.

use std::collections::BTreeSet;
use std::sync::Arc;

use super::program::{Inst, Program};
use crate::parse::{END_OF_INPUT, Limit, Limits};
use crate::tree::{FieldId, KindId, Tree};

/// An item is a token.
const TOKEN: u8 = 1;
/// A token is hidden.
const HIDDEN: u8 = 2;
/// A node holds a token at some depth, so it counts for the span of its parent.
const HAS_TOKEN: u8 = 4;

/// A token, or a node closed into the arena, waiting to become a child.
#[derive(Clone, Copy, Debug)]
struct Item {
    /// The kind of a token, or the arena index of a node.
    value: u32,
    start: u32,
    end: u32,
    field: FieldId,
    flags: u8,
}

impl Item {
    fn is_hidden_token(&self) -> bool {
        self.flags & (TOKEN | HIDDEN) == TOKEN | HIDDEN
    }
}

/// A closed node: its kind and its children, a range of [`Machine::children`].
#[derive(Clone, Copy, Debug)]
struct Node {
    kind: KindId,
    first: u32,
    count: u32,
}

/// An entry of the machine's stack. `items` is the height of the item stack when the entry was
/// pushed. See [`Inst`] for what pushes and pops each.
#[derive(Clone, Copy, Debug)]
enum Entry {
    Choice {
        alt: u32,
        pos: u32,
        items: u32,
    },
    Label {
        alt: u32,
        pos: u32,
        items: u32,
    },
    Predicate {
        alt: u32,
        pos: u32,
        items: u32,
        /// The quiet count before the lookahead started.
        quiet: u32,
    },
    Repetition {
        end: u32,
        /// Where the current step started.
        pos: u32,
        items: u32,
        /// Steps still needed to reach the minimum.
        need: u32,
        /// Steps still allowed, `u32::MAX` without a maximum.
        left: u32,
    },
    Call {
        ret: u32,
    },
    Sub {
        ret: u32,
    },
    Open {
        pos: u32,
        items: u32,
    },
    Mark {
        pos: u32,
    },
    Flags {
        saved: u64,
    },
}

/// The token being matched: where it started, its kind, and how many entries the stack held
/// when it opened. Tokens never nest, since nothing inside a token makes one.
#[derive(Clone, Copy, Debug)]
struct OpenToken {
    start: u32,
    kind: KindId,
    height: u32,
}

/// Why the machine stopped without a tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    /// The input does not match.
    NoMatch,
    /// A labeled error, with the index of its label and its position.
    Error {
        label: u32,
        position: u32,
    },
    Limit(Limit),
}

/// What a failure expects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Expected {
    Kind(KindId),
    EndOfInput,
}

pub(crate) struct Machine<'p, 'i> {
    program: &'p Program,
    input: &'i str,
    pos: u32,
    pc: u32,
    stack: Vec<Entry>,
    items: Vec<Item>,
    nodes: Vec<Node>,
    children: Vec<Item>,
    token: Option<OpenToken>,
    flags: u64,
    /// Failures are recorded only while this is 0: outside lookahead and the skip routine.
    quiet: u32,
    depth: u32,
    steps: u64,
    limits: Limits,
    /// The farthest position a failure was recorded at.
    farthest: Option<u32>,
    /// In diagnose mode, the farthest failure position of the run before, and what failed there.
    diagnose: Option<(u32, BTreeSet<Expected>)>,
}

impl<'p, 'i> Machine<'p, 'i> {
    /// A machine for one parse. The input must fit 32-bit offsets. With `diagnose`, it collects
    /// what fails at that position.
    pub fn new(
        program: &'p Program,
        input: &'i str,
        limits: Limits,
        diagnose: Option<u32>,
    ) -> Machine<'p, 'i> {
        Machine {
            program,
            input,
            pos: 0,
            pc: 0,
            stack: Vec::new(),
            items: Vec::new(),
            nodes: Vec::new(),
            children: Vec::new(),
            token: None,
            flags: 0,
            quiet: 0,
            depth: 0,
            steps: 0,
            limits,
            farthest: None,
            diagnose: diagnose.map(|at| (at, BTreeSet::new())),
        }
    }

    /// The farthest position a failure was recorded at.
    pub fn farthest(&self) -> Option<u32> {
        self.farthest
    }

    /// In diagnose mode, what failed at the position, as an expected set shows it, sorted.
    pub fn expected(&self) -> Vec<String> {
        let Some((_, expected)) = &self.diagnose else {
            return Vec::new();
        };
        let shown: BTreeSet<String> = (expected.iter())
            .map(|what| match what {
                Expected::Kind(kind) => (self.program.kind_display)
                    .get(kind.0 as usize)
                    .cloned()
                    .unwrap_or_default(),
                Expected::EndOfInput => END_OF_INPUT.to_owned(),
            })
            .collect();
        shown.into_iter().collect()
    }

    fn rest(&self) -> &'i str {
        self.input.get(self.pos as usize..).unwrap_or("")
    }

    /// Consume `len` bytes and go on, or fail when there is no match.
    fn advance(&mut self, len: Option<usize>) -> Result<(), Stop> {
        match len {
            // A match lies within the input, whose length fits 32 bits.
            Some(len) => {
                self.pos += len as u32;
                self.pc += 1;
                Ok(())
            }
            None => self.fail(),
        }
    }

    /// Go on when the test holds, or fail.
    fn test(&mut self, holds: bool) -> Result<(), Stop> {
        if holds {
            self.pc += 1;
            Ok(())
        } else {
            self.fail()
        }
    }

    /// Run the routine at `entry` until it ends the parse.
    pub fn run(&mut self, entry: u32) -> Result<(), Stop> {
        self.pc = entry;
        loop {
            self.steps += 1;
            if self.steps > self.limits.max_steps {
                return Err(Stop::Limit(Limit::Steps));
            }
            // The verifier holds every jump inside the program.
            let Some(&inst) = self.program.code.get(self.pc as usize) else {
                return Err(Stop::NoMatch);
            };
            match inst {
                Inst::Char(byte) => {
                    let found = self.rest().as_bytes().first() == Some(&byte);
                    self.advance(found.then_some(1))?;
                }
                Inst::Str(index) => {
                    let literal = self.literal(index);
                    let found = self.rest().as_bytes().starts_with(literal);
                    self.advance(found.then_some(literal.len()))?;
                }
                Inst::StrFold(index) => {
                    let literal = self.literal(index);
                    let rest = self.rest().as_bytes();
                    let found = (rest.get(..literal.len()))
                        .is_some_and(|r| r.eq_ignore_ascii_case(literal));
                    self.advance(found.then_some(literal.len()))?;
                }
                Inst::Class(index) => {
                    let len = (self.program.classes.get(index as usize))
                        .and_then(|class| class.matches(self.rest()));
                    self.advance(len)?;
                }
                Inst::Any => {
                    let len = self.rest().chars().next().map(char::len_utf8);
                    self.advance(len)?;
                }
                Inst::Choice(alt) => {
                    self.push(|pos, items| Entry::Choice { alt, pos, items });
                }
                Inst::LabelChoice(alt) => {
                    self.push(|pos, items| Entry::Label { alt, pos, items });
                }
                Inst::PredChoice(alt) => {
                    let quiet = self.quiet;
                    self.push(|pos, items| Entry::Predicate {
                        alt,
                        pos,
                        items,
                        quiet,
                    });
                    self.quiet += 1;
                }
                Inst::Commit(target) => {
                    self.stack.pop();
                    self.pc = target;
                }
                Inst::LoopCommit(target) => match self.stack.last_mut() {
                    Some(Entry::Choice { pos, items, .. }) if self.pos > *pos => {
                        *pos = self.pos;
                        *items = self.items.len() as u32;
                        self.pc = target;
                    }
                    _ => {
                        self.stack.pop();
                        self.pc += 1;
                    }
                },
                Inst::RepOpen { end, bounds } => {
                    let (need, left) = (self.program.bounds.get(bounds as usize))
                        .copied()
                        .unwrap_or((0, 1));
                    self.push(|pos, items| Entry::Repetition {
                        end,
                        pos,
                        items,
                        need,
                        left,
                    });
                }
                Inst::RepNext(target) => match self.stack.last_mut() {
                    Some(Entry::Repetition {
                        pos,
                        items,
                        need,
                        left,
                        ..
                    }) if self.pos > *pos => {
                        *need = need.saturating_sub(1);
                        if *left != u32::MAX {
                            *left -= 1;
                        }
                        if *left == 0 {
                            self.stack.pop();
                            self.pc += 1;
                        } else {
                            *pos = self.pos;
                            *items = self.items.len() as u32;
                            self.pc = target;
                        }
                    }
                    // A step that consumes nothing is the last, and the repetition succeeds.
                    _ => {
                        self.stack.pop();
                        self.pc += 1;
                    }
                },
                Inst::BackCommit(target) => {
                    if let Some(Entry::Predicate {
                        pos, items, quiet, ..
                    }) = self.stack.pop()
                    {
                        self.pos = pos;
                        self.items.truncate(items as usize);
                        self.quiet = quiet;
                    }
                    self.pc = target;
                }
                Inst::FailTwice => {
                    if let Some(Entry::Predicate { quiet, .. }) = self.stack.pop() {
                        self.quiet = quiet;
                    }
                    self.fail()?;
                }
                Inst::Throw(label) => self.throw(label)?,
                Inst::Fail => self.fail()?,
                Inst::Call(target) => {
                    if self.depth >= self.limits.max_depth {
                        return Err(Stop::Limit(Limit::Depth));
                    }
                    self.depth += 1;
                    self.stack.push(Entry::Call { ret: self.pc + 1 });
                    self.pc = target;
                }
                Inst::Sub(target) => {
                    self.stack.push(Entry::Sub { ret: self.pc + 1 });
                    self.pc = target;
                }
                Inst::Skip => match self.program.skip {
                    Some(skip) => {
                        self.stack.push(Entry::Sub { ret: self.pc + 1 });
                        self.pc = skip;
                    }
                    None => self.pc += 1,
                },
                Inst::Ret => match self.stack.pop() {
                    Some(Entry::Call { ret }) => {
                        self.depth -= 1;
                        self.pc = ret;
                    }
                    Some(Entry::Sub { ret }) => self.pc = ret,
                    _ => return Err(Stop::NoMatch),
                },
                Inst::End => {
                    if self.pos as usize == self.input.len() {
                        return Ok(());
                    }
                    self.record(self.pos, Expected::EndOfInput);
                    return Err(Stop::NoMatch);
                }
                Inst::QuietOn => {
                    self.quiet += 1;
                    self.pc += 1;
                }
                Inst::QuietOff => {
                    self.quiet = self.quiet.saturating_sub(1);
                    self.pc += 1;
                }
                Inst::TokenOpen(kind) => {
                    self.token = Some(OpenToken {
                        start: self.pos,
                        kind,
                        height: self.stack.len() as u32,
                    });
                    self.pc += 1;
                }
                Inst::TokenClose { hidden } => {
                    if let Some(token) = self.token.take() {
                        self.items.push(Item {
                            value: token.kind.0,
                            start: token.start,
                            end: self.pos,
                            field: FieldId::NONE,
                            flags: TOKEN | if hidden { HIDDEN } else { 0 },
                        });
                    }
                    self.pc += 1;
                }
                Inst::Open => self.push(|pos, items| Entry::Open { pos, items }),
                Inst::CloseNode(kind) => {
                    if let Some(Entry::Open { pos, items }) = self.stack.pop() {
                        self.close_node(kind, pos, items as usize)?;
                    }
                    self.pc += 1;
                }
                Inst::CloseTransparent(kind) => {
                    if let Some(Entry::Open { pos, items }) = self.stack.pop() {
                        let collected = self.items.get(items as usize..).unwrap_or_default();
                        self.steps += collected.len() as u64;
                        let visible = collected.iter().filter(|i| !i.is_hidden_token()).count();
                        if visible != 1 {
                            self.close_node(kind, pos, items as usize)?;
                        }
                    }
                    self.pc += 1;
                }
                Inst::CloseField(field) => {
                    if let Some(Entry::Open { items, .. }) = self.stack.pop() {
                        let collected = self.items.get_mut(items as usize..).unwrap_or_default();
                        self.steps += collected.len() as u64;
                        for item in collected {
                            if item.field == FieldId::NONE && !item.is_hidden_token() {
                                item.field = field;
                            }
                        }
                    }
                    self.pc += 1;
                }
                Inst::WordEnd => {
                    let word = self.program.word.matches(self.rest()).is_some();
                    self.test(!word)?;
                }
                Inst::Mark => {
                    self.stack.push(Entry::Mark { pos: self.pos });
                    self.pc += 1;
                }
                Inst::Unreserved => {
                    let start = match self.stack.pop() {
                        Some(Entry::Mark { pos }) => pos,
                        _ => self.pos,
                    };
                    let text = self.input.get(start as usize..self.pos as usize);
                    let text = text.unwrap_or_default();
                    // Looking a match up is work of its length.
                    self.steps += text.len() as u64;
                    let reserved = self.program.reserved.contains(text);
                    self.test(!reserved)?;
                }
                Inst::FlagOn(flag) => {
                    self.stack.push(Entry::Flags { saved: self.flags });
                    self.flags |= 1 << flag;
                    self.pc += 1;
                }
                Inst::FlagOff(flag) => {
                    self.stack.push(Entry::Flags { saved: self.flags });
                    self.flags &= !(1 << flag);
                    self.pc += 1;
                }
                Inst::FlagsPop => {
                    if let Some(Entry::Flags { saved }) = self.stack.pop() {
                        self.flags = saved;
                    }
                    self.pc += 1;
                }
                Inst::FlagTest { flag, negated } => {
                    let on = self.flags >> flag & 1 == 1;
                    self.test(on != negated)?;
                }
            }
        }
    }

    fn literal(&self, index: u32) -> &'p [u8] {
        self.program
            .strings
            .get(index as usize)
            .map_or(&[], |literal| literal)
    }

    /// Push an entry made from the position and the height of the item stack, and go on.
    fn push(&mut self, entry: impl FnOnce(u32, u32) -> Entry) {
        self.stack.push(entry(self.pos, self.items.len() as u32));
        self.pc += 1;
    }

    /// Unwind the stack to the newest entry that catches a failure and resume there, or stop
    /// when none does.
    fn fail(&mut self) -> Result<(), Stop> {
        while let Some(entry) = self.stack.pop() {
            let caught = match entry {
                Entry::Choice { alt, pos, items } | Entry::Label { alt, pos, items } => {
                    Some((alt, pos, items, None))
                }
                Entry::Predicate {
                    alt,
                    pos,
                    items,
                    quiet,
                } => Some((alt, pos, items, Some(quiet))),
                // A repetition that made enough steps succeeds with them.
                Entry::Repetition {
                    end,
                    pos,
                    items,
                    need: 0,
                    ..
                } => Some((end, pos, items, None)),
                Entry::Repetition { .. } | Entry::Sub { .. } | Entry::Open { .. } => None,
                Entry::Mark { .. } => None,
                Entry::Call { .. } => {
                    self.depth -= 1;
                    None
                }
                Entry::Flags { saved } => {
                    self.flags = saved;
                    None
                }
            };
            if let Some((alt, pos, items, quiet)) = caught {
                self.end_failed_token();
                self.pos = pos;
                self.items.truncate(items as usize);
                if let Some(quiet) = quiet {
                    self.quiet = quiet;
                }
                self.pc = alt;
                return Ok(());
            }
        }
        self.end_failed_token();
        Err(Stop::NoMatch)
    }

    /// The open token failed when the entry that caught the failure is older than the token.
    /// Its start is recorded then, with the quiet count of the failure.
    fn end_failed_token(&mut self) {
        if let Some(token) = self.token
            && (self.stack.len() as u32) < token.height
        {
            self.token = None;
            self.record(token.start, Expected::Kind(token.kind));
        }
    }

    /// Stop the parse with a labeled error, unless a lookahead encloses it: then the lookahead
    /// fails, and a token the error passed through ends without a failure being recorded.
    fn throw(&mut self, label: u32) -> Result<(), Stop> {
        let position = self.pos;
        while let Some(entry) = self.stack.pop() {
            match entry {
                Entry::Predicate {
                    alt,
                    pos,
                    items,
                    quiet,
                } => {
                    if self
                        .token
                        .is_some_and(|token| (self.stack.len() as u32) < token.height)
                    {
                        self.token = None;
                    }
                    self.pos = pos;
                    self.items.truncate(items as usize);
                    self.quiet = quiet;
                    self.pc = alt;
                    return Ok(());
                }
                Entry::Call { .. } => self.depth -= 1,
                Entry::Flags { saved } => self.flags = saved,
                _ => {}
            }
        }
        Err(Stop::Error { label, position })
    }

    fn record(&mut self, pos: u32, what: Expected) {
        if self.quiet > 0 {
            return;
        }
        if self.farthest.is_none_or(|farthest| pos > farthest) {
            self.farthest = Some(pos);
        }
        if let Some((at, expected)) = &mut self.diagnose
            && pos == *at
        {
            expected.insert(what);
        }
    }

    /// Make the items above `height` a node: they move to the arena, and one item for the node
    /// takes their place. Its span runs from its first token to its last at any depth, or is
    /// empty at `pos` when it holds none.
    fn close_node(&mut self, kind: KindId, pos: u32, height: usize) -> Result<(), Stop> {
        let collected = self.items.get(height..).unwrap_or_default();
        self.steps += collected.len() as u64;
        let mut span: Option<(u32, u32)> = None;
        for item in collected {
            if item.flags & (TOKEN | HAS_TOKEN) != 0 {
                span = Some((span.map_or(item.start, |(start, _)| start), item.end));
            }
        }
        let (start, end, flags) = match span {
            Some((start, end)) => (start, end, HAS_TOKEN),
            None => (pos, pos, 0),
        };
        // Indices are 32 bits wide. Every node costs steps, so a limit of steps stops first.
        let (Ok(index), Ok(first)) = (
            u32::try_from(self.nodes.len()),
            u32::try_from(self.children.len()),
        ) else {
            return Err(Stop::Limit(Limit::Steps));
        };
        self.children.extend_from_slice(collected);
        self.nodes.push(Node {
            kind,
            first,
            count: collected.len() as u32,
        });
        self.items.truncate(height);
        self.items.push(Item {
            value: index,
            start,
            end,
            field: FieldId::NONE,
            flags,
        });
        Ok(())
    }

    /// Lay out the items of a successful parse as the top level of a tree, in postorder.
    pub fn tree(&self) -> Tree {
        let mut tree = Tree::empty(Arc::clone(&self.program.names));
        // The nodes being laid out, with the index of the next child of each and the length of
        // the tree when it started. An explicit stack, since trees can be as deep as long.
        let mut open: Vec<(Item, u32, usize)> = Vec::new();
        for &top in &self.items {
            open.push((top, 0, tree.len()));
            while let Some(&(item, next, started)) = open.last() {
                if item.flags & TOKEN != 0 {
                    open.pop();
                    let hidden = item.flags & HIDDEN != 0;
                    tree.push_token(KindId(item.value), hidden, item.start, item.end, item.field);
                    continue;
                }
                let Some(&node) = self.nodes.get(item.value as usize) else {
                    open.pop();
                    continue;
                };
                if next < node.count {
                    if let Some(last) = open.last_mut() {
                        last.1 += 1;
                    }
                    if let Some(&child) = self.children.get((node.first + next) as usize) {
                        open.push((child, 0, tree.len()));
                    }
                } else {
                    open.pop();
                    let size = (tree.len() - started + 1) as u32;
                    tree.push_node(node.kind, item.start, item.end, size, item.field);
                }
            }
        }
        tree
    }
}
