// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The parsing machine: runs a program over an input and lays out the tree it built.
//!
//! The machine keeps one stack of entries on the heap, so a deep grammar never touches the
//! caller's stack. Items of nodes being built wait on a stack of their own, and a closed node
//! moves its items to the arena. The entry that catches a failure truncates both to where they
//! stood when it was pushed, so a parse holds no more nodes than its alternatives in progress
//! make, and those that remembered results hold: no failure cuts the arena below its length
//! at the newest result remembered with a node.
//!
//! A call of a rule that is remembered or on a left cycle keeps a frame beside the stack.
//! Left recursion follows the semantics page: a frame is the entry of the table of left
//! recursion for its rule, position and flags, a call that finds it takes the match of the
//! pass before, its seed, and the rule's routine runs again for as long as each pass ends
//! further.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use super::program::{Inst, Program};
use crate::parse::{END_OF_INPUT, Limit, Limits};
use crate::tree::{FieldId, KindId, Tree};

/// Bytes of a literal compared in one step, about the work of one instruction.
const LITERAL_BYTES_PER_STEP: u64 = 8;

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

/// What an entry that catches a failure gives back: the position, the height of the item
/// stack and the length of the arena when it was pushed.
#[derive(Clone, Copy, Debug)]
struct Saved {
    pos: u32,
    items: u32,
    nodes: u32,
}

/// An entry of the machine's stack. See [`Inst`] for what pushes and pops each.
#[derive(Clone, Copy, Debug)]
enum Entry {
    Choice {
        alt: u32,
        saved: Saved,
        /// A cut marked the entry: a failure drops it and unwinds on.
        cut: bool,
    },
    Label {
        alt: u32,
        saved: Saved,
    },
    Predicate {
        alt: u32,
        saved: Saved,
        /// The quiet count before the lookahead started.
        quiet: u32,
    },
    Repetition {
        end: u32,
        /// Where the current step started.
        saved: Saved,
        /// Steps still needed to reach the minimum.
        need: u32,
        /// Steps still allowed, `u32::MAX` without a maximum.
        left: u32,
        /// A cut marked the entry, as for a choice.
        cut: bool,
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
    /// A call of a rule that is remembered or on a left cycle. What it needs is the newest of
    /// [`Machine::frames`], so that the entries keep their size.
    Frame,
}

/// What a remembered result depends on: the rule, the position, the flags that are on, and
/// whether failures were being recorded, since a result found while they were not recorded none.
/// Without the last, it is the key of the table of left recursion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct MemoKey {
    rule: u32,
    pos: u32,
    flags: u64,
    quiet: bool,
}

/// A match kept for later: where it ended and the items it left, a range of the items kept.
#[derive(Clone, Copy, Debug)]
struct Match {
    end: u32,
    first: u32,
    count: u32,
}

impl Match {
    fn range(&self) -> std::ops::Range<usize> {
        self.first as usize..(self.first + self.count) as usize
    }
}

/// A remembered result: a match with its items in [`Machine::memo_items`], or a failure, and
/// the first serial not yet given to a frame of left recursion when it was stored.
#[derive(Clone, Copy, Debug)]
struct Stored {
    result: Option<Match>,
    made: u64,
}

/// What a frame of left recursion adds to a call: its entry of the table of left recursion.
#[derive(Clone, Copy, Debug)]
struct Recursion {
    /// The match of the pass before, with its items in [`Machine::seeds`], which a use of the
    /// entry gives. None until the body matched once, and then a use fails.
    seed: Option<Match>,
    /// The length of the arena after the seed, whose nodes a pass that fails must leave.
    seed_nodes: u32,
    /// Whether the pass in progress used the entry.
    used: bool,
    /// Frames of left recursion are numbered as they are made.
    serial: u64,
    /// The frame of left recursion made before this one, an index of [`Machine::frames`].
    older: u32,
}

/// A call in progress of a rule that is remembered or on a left cycle.
#[derive(Clone, Copy, Debug)]
struct Frame {
    ret: u32,
    key: MemoKey,
    /// The height of the item stack when the call started.
    items: u32,
    /// Whether the result is to be remembered.
    memo: bool,
    recursion: Option<Recursion>,
    /// The oldest frame of left recursion that the evaluation around the call had used.
    outer_dependency: u32,
}

/// No frame, where an index of [`Machine::frames`] is expected.
const NO_FRAME: u32 = u32::MAX;

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
    /// The calls in progress of rules that are remembered or on a left cycle, one for each
    /// `Frame` entry of the stack.
    frames: Vec<Frame>,
    /// The items of the seeds of the frames of left recursion, the newest frame's last.
    seeds: Vec<Item>,
    /// The newest frame of left recursion, or [`NO_FRAME`].
    newest_recursion: u32,
    /// The serial of the next frame of left recursion.
    serial: u64,
    /// The oldest frame of left recursion whose entry the evaluation in progress used, or
    /// [`NO_FRAME`]. A result that used a frame older than its own call is not remembered.
    dependency: u32,
    memo: HashMap<MemoKey, Stored>,
    /// The items of the remembered matches.
    memo_items: Vec<Item>,
    /// The length of the arena at the newest remembered match that holds a node. The nodes
    /// below it stay, since remembered items point at them.
    protected: u32,
    /// Whether a remembered match was given again, so that two parents may hold one node.
    shared: bool,
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
            frames: Vec::new(),
            seeds: Vec::new(),
            newest_recursion: NO_FRAME,
            serial: 0,
            dependency: NO_FRAME,
            memo: HashMap::new(),
            memo_items: Vec::new(),
            protected: 0,
            shared: false,
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

    /// How many nodes the arena holds.
    #[cfg(test)]
    pub fn arena_len(&self) -> usize {
        self.nodes.len()
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
                    self.steps += literal.len() as u64 / LITERAL_BYTES_PER_STEP;
                    let found = self.rest().as_bytes().starts_with(literal);
                    self.advance(found.then_some(literal.len()))?;
                }
                Inst::StrFold(index) => {
                    let literal = self.literal(index);
                    self.steps += literal.len() as u64 / LITERAL_BYTES_PER_STEP;
                    let rest = self.rest().as_bytes();
                    let found = (rest.get(..literal.len()))
                        .is_some_and(|r| r.eq_ignore_ascii_case(literal));
                    self.advance(found.then_some(literal.len()))?;
                }
                Inst::Class(index) => {
                    let len = (self.program.classes.get(index as usize))
                        .and_then(|class| class.matches(self.rest(), &mut self.steps));
                    self.advance(len)?;
                }
                Inst::Any => {
                    let len = self.rest().chars().next().map(char::len_utf8);
                    self.advance(len)?;
                }
                Inst::Choice(alt) => self.push(Entry::Choice {
                    alt,
                    saved: self.saved(),
                    cut: false,
                }),
                Inst::LabelChoice(alt) => self.push(Entry::Label {
                    alt,
                    saved: self.saved(),
                }),
                Inst::PredChoice(alt) => {
                    self.push(Entry::Predicate {
                        alt,
                        saved: self.saved(),
                        quiet: self.quiet,
                    });
                    self.quiet += 1;
                }
                Inst::Commit(target) => {
                    self.stack.pop();
                    self.pc = target;
                }
                Inst::LoopCommit(target) => match self.stack.last_mut() {
                    Some(Entry::Choice { saved, cut, .. }) if self.pos > saved.pos => {
                        *saved = Saved {
                            pos: self.pos,
                            items: self.items.len() as u32,
                            nodes: self.nodes.len() as u32,
                        };
                        // A cut inside a step that succeeded is spent.
                        *cut = false;
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
                    self.push(Entry::Repetition {
                        end,
                        saved: self.saved(),
                        need,
                        left,
                        cut: false,
                    });
                }
                Inst::RepNext(target) => match self.stack.last_mut() {
                    Some(Entry::Repetition {
                        saved,
                        need,
                        left,
                        cut,
                        ..
                    }) if self.pos > saved.pos => {
                        *cut = false;
                        *need = need.saturating_sub(1);
                        if *left != u32::MAX {
                            *left -= 1;
                        }
                        if *left == 0 {
                            self.stack.pop();
                            self.pc += 1;
                        } else {
                            *saved = Saved {
                                pos: self.pos,
                                items: self.items.len() as u32,
                                nodes: self.nodes.len() as u32,
                            };
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
                    if let Some(Entry::Predicate { saved, quiet, .. }) = self.stack.pop() {
                        self.restore(saved);
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
                Inst::Cut => {
                    self.cut();
                    self.pc += 1;
                }
                Inst::Call(target) => {
                    if self.depth >= self.limits.max_depth {
                        return Err(Stop::Limit(Limit::Depth));
                    }
                    self.depth += 1;
                    self.stack.push(Entry::Call { ret: self.pc + 1 });
                    self.pc = target;
                }
                Inst::MemoCall(rule) => self.call_frame(rule, false)?,
                Inst::LrCall(rule) => self.call_frame(rule, true)?,
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
                    Some(Entry::Frame) => self.return_frame()?,
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
                Inst::Open => self.push(Entry::Open {
                    pos: self.pos,
                    items: self.items.len() as u32,
                }),
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
                    let rest = self.rest();
                    let word = self.program.word.matches(rest, &mut self.steps).is_some();
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

    /// Push an entry and go on.
    fn push(&mut self, entry: Entry) {
        self.stack.push(entry);
        self.pc += 1;
    }

    /// What an entry pushed now gives back when it catches a failure.
    fn saved(&self) -> Saved {
        Saved {
            pos: self.pos,
            items: self.items.len() as u32,
            nodes: self.nodes.len() as u32,
        }
    }

    /// Call a rule that is remembered or on a left cycle: give what the table of left recursion
    /// or the memo holds for it here, or push a frame and run its routine.
    fn call_frame(&mut self, rule: u32, recursive: bool) -> Result<(), Stop> {
        // The verifier holds the index inside the rules.
        let Some(info) = self.program.rules.get(rule as usize) else {
            return Err(Stop::NoMatch);
        };
        if recursive && let Some(index) = self.active(rule) {
            return self.use_entry(index);
        }
        let key = MemoKey {
            rule,
            pos: self.pos,
            flags: self.flags,
            quiet: self.quiet > 0,
        };
        if info.memo
            && let Some(stored) = self.memo.get(&key).copied()
            && self.usable(stored)
        {
            let Some(found) = stored.result else {
                return self.fail();
            };
            // Giving a result again is work of its length, and may put it under two parents.
            self.steps += u64::from(found.count);
            self.shared = true;
            let items = self.memo_items.get(found.range()).unwrap_or_default();
            self.items.extend_from_slice(items);
            self.pos = found.end;
            self.pc += 1;
            return Ok(());
        }
        if self.depth >= self.limits.max_depth {
            return Err(Stop::Limit(Limit::Depth));
        }
        self.depth += 1;
        // Frames nest no deeper than calls, which the depth limit counts in 32 bits.
        let index = self.frames.len() as u32;
        let recursion = recursive.then_some(Recursion {
            seed: None,
            seed_nodes: 0,
            used: false,
            serial: self.serial,
            older: self.newest_recursion,
        });
        if recursive {
            self.newest_recursion = index;
            self.serial += 1;
        }
        self.frames.push(Frame {
            ret: self.pc + 1,
            key,
            items: self.items.len() as u32,
            memo: info.memo,
            recursion,
            outer_dependency: std::mem::replace(&mut self.dependency, NO_FRAME),
        });
        self.stack.push(Entry::Frame);
        self.pc = info.pc;
        Ok(())
    }

    /// The frame of `rule` that started at this position with these flags, if there is one:
    /// its entry of the table of left recursion. The frames that started here are the newest,
    /// so the search ends at the first that started earlier.
    fn active(&mut self, rule: u32) -> Option<u32> {
        let mut index = self.newest_recursion;
        while let Some(frame) = self.frames.get(index as usize) {
            if frame.key.pos != self.pos {
                break;
            }
            if frame.key.rule == rule && frame.key.flags == self.flags {
                return Some(index);
            }
            self.steps += 1;
            index = frame
                .recursion
                .map_or(NO_FRAME, |recursion| recursion.older);
        }
        None
    }

    /// A left-recursive use: give the seed of the frame, or fail while it has none. A pass can
    /// use a seed twice only while the seed is empty, since after a seed that consumed input
    /// the position is another. So a seed doubles no tree that grows, and unlike a remembered
    /// match it needs no count of the tree's entries.
    fn use_entry(&mut self, index: u32) -> Result<(), Stop> {
        self.dependency = self.dependency.min(index);
        let frame = self.frames.get_mut(index as usize);
        let Some(recursion) = frame.and_then(|frame| frame.recursion.as_mut()) else {
            return Err(Stop::NoMatch);
        };
        recursion.used = true;
        let Some(seed) = recursion.seed else {
            return self.fail();
        };
        self.steps += u64::from(seed.count);
        let items = self.seeds.get(seed.range()).unwrap_or_default();
        self.items.extend_from_slice(items);
        self.pos = seed.end;
        self.pc += 1;
        Ok(())
    }

    /// Whether a remembered result may be given here. Its evaluation used no frame of left
    /// recursion still in progress, but matching again could use one made since, and only one
    /// that started at this position. Those are the newest, so the newest decides.
    fn usable(&self, stored: Stored) -> bool {
        let newest = self.frames.get(self.newest_recursion as usize);
        match newest.and_then(|frame| Some((frame.key.pos, frame.recursion?.serial))) {
            Some((pos, serial)) => pos < self.pos || serial < stored.made,
            None => true,
        }
    }

    /// The body of the newest frame matched. A frame of left recursion whose entry the pass
    /// used keeps the match as its seed and runs the body again, for as long as each pass ends
    /// further than the one before. Otherwise the call returns.
    fn return_frame(&mut self) -> Result<(), Stop> {
        let Some(&frame) = self.frames.last() else {
            return Err(Stop::NoMatch);
        };
        if let Some(recursion) = frame.recursion
            && recursion.used
        {
            match recursion.seed {
                Some(seed) if self.pos <= seed.end => {
                    self.back_to_seed(frame, seed, recursion.seed_nodes);
                }
                _ => return self.grow(frame, recursion),
            }
        }
        self.end_frame(true)?;
        self.pc = frame.ret;
        Ok(())
    }

    /// Keep the match of a pass as the seed of the newest frame, and run its body again.
    fn grow(&mut self, frame: Frame, recursion: Recursion) -> Result<(), Stop> {
        let Some(info) = self.program.rules.get(frame.key.rule as usize) else {
            return Err(Stop::NoMatch);
        };
        let made = self.items.get(frame.items as usize..).unwrap_or_default();
        self.steps += made.len() as u64;
        let first = recursion
            .seed
            .map_or(self.seeds.len(), |s| s.first as usize);
        self.seeds.truncate(first);
        // Indices are 32 bits wide. Every kept item costs a step, so a limit of steps stops
        // first.
        let (Ok(first), Ok(seed_nodes)) = (u32::try_from(first), u32::try_from(self.nodes.len()))
        else {
            return Err(Stop::Limit(Limit::Steps));
        };
        self.seeds.extend_from_slice(made);
        let seed = Match {
            end: self.pos,
            first,
            count: made.len() as u32,
        };
        self.items.truncate(frame.items as usize);
        self.pos = frame.key.pos;
        if let Some(newest) = self.frames.last_mut() {
            newest.recursion = Some(Recursion {
                seed: Some(seed),
                seed_nodes,
                used: false,
                ..recursion
            });
        }
        self.stack.push(Entry::Frame);
        self.pc = info.pc;
        Ok(())
    }

    /// Make the seed of a frame the result of its call: the items and the arena as they stood
    /// after the seed, whose nodes lie below everything a later pass made.
    fn back_to_seed(&mut self, frame: Frame, seed: Match, seed_nodes: u32) {
        self.restore(Saved {
            pos: seed.end,
            items: frame.items,
            nodes: seed_nodes,
        });
        let items = self.seeds.get(seed.range()).unwrap_or_default();
        self.items.extend_from_slice(items);
    }

    /// Take the newest frame off: out of the table of left recursion, with its seed. Returns
    /// it, and whether its evaluation used no frame older than itself.
    fn drop_frame(&mut self) -> Option<(Frame, bool)> {
        let frame = self.frames.pop()?;
        let index = self.frames.len() as u32;
        self.depth -= 1;
        if let Some(recursion) = frame.recursion {
            self.newest_recursion = recursion.older;
            if let Some(seed) = recursion.seed {
                self.seeds.truncate(seed.first as usize);
            }
        }
        let used = self.dependency;
        self.dependency = frame.outer_dependency.min(used);
        Some((frame, used >= index))
    }

    /// Take the newest frame off and remember its result: the items above its height if it
    /// `matched`, else a failure. A result that used a frame older than its own call depends on
    /// a match that is still growing and is not remembered. A match that holds a node keeps the
    /// arena as long as it is now.
    fn end_frame(&mut self, matched: bool) -> Result<(), Stop> {
        let Some((frame, independent)) = self.drop_frame() else {
            return Ok(());
        };
        if !(frame.memo && independent) {
            return Ok(());
        }
        let mut result = None;
        if matched {
            let made = self.items.get(frame.items as usize..).unwrap_or_default();
            self.steps += made.len() as u64;
            // Indices are 32 bits wide. Every stored item costs a step, so a limit of steps
            // stops first.
            let (Ok(first), Ok(nodes)) = (
                u32::try_from(self.memo_items.len()),
                u32::try_from(self.nodes.len()),
            ) else {
                return Err(Stop::Limit(Limit::Steps));
            };
            if made.iter().any(|item| item.flags & TOKEN == 0) {
                self.protected = nodes;
            }
            self.memo_items.extend_from_slice(made);
            result = Some(Match {
                end: self.pos,
                first,
                count: made.len() as u32,
            });
        }
        let made = self.serial;
        self.memo.insert(frame.key, Stored { result, made });
        Ok(())
    }

    /// Go back to what an entry saved. The nodes made since lie at the end of the arena, and
    /// their children at the end of the children, so both are cut back, as far as no
    /// remembered match holds them.
    fn restore(&mut self, saved: Saved) {
        self.pos = saved.pos;
        self.items.truncate(saved.items as usize);
        self.nodes
            .truncate(saved.nodes.max(self.protected) as usize);
        let children = self.nodes.last().map_or(0, |node| node.first + node.count);
        self.children.truncate(children as usize);
    }

    /// Unwind the stack to the newest entry that catches a failure and resume there, or stop
    /// when none does.
    fn fail(&mut self) -> Result<(), Stop> {
        while let Some(entry) = self.stack.pop() {
            let caught = match entry {
                // A cut made the failure pass the entry, so what it guards fails as a whole.
                Entry::Choice { cut: true, .. } | Entry::Repetition { cut: true, .. } => None,
                Entry::Choice { alt, saved, .. } | Entry::Label { alt, saved } => {
                    Some((alt, saved, None))
                }
                Entry::Predicate { alt, saved, quiet } => Some((alt, saved, Some(quiet))),
                // A repetition that made enough steps succeeds with them.
                Entry::Repetition {
                    end,
                    saved,
                    need: 0,
                    ..
                } => Some((end, saved, None)),
                Entry::Repetition { .. } | Entry::Sub { .. } | Entry::Open { .. } => None,
                Entry::Mark { .. } => None,
                Entry::Call { .. } => {
                    self.depth -= 1;
                    None
                }
                Entry::Frame => {
                    let recursion = self.frames.last().and_then(|frame| frame.recursion);
                    if let (Some(&frame), Some(recursion)) = (self.frames.last(), recursion)
                        && let Some(seed) = recursion.seed
                    {
                        // A pass that fails leaves the seed as the result of the call.
                        self.end_failed_token(Some(self.stack.len()));
                        self.back_to_seed(frame, seed, recursion.seed_nodes);
                        self.end_frame(true)?;
                        self.pc = frame.ret;
                        return Ok(());
                    }
                    self.end_frame(false)?;
                    None
                }
                Entry::Flags { saved } => {
                    self.flags = saved;
                    None
                }
            };
            if let Some((alt, saved, quiet)) = caught {
                self.end_failed_token(Some(self.stack.len()));
                self.restore(saved);
                if let Some(quiet) = quiet {
                    self.quiet = quiet;
                }
                self.pc = alt;
                return Ok(());
            }
        }
        // Nothing caught the failure, so the open token failed, even one opened on an empty
        // stack, as a parse from a terminal opens its token.
        self.end_failed_token(None);
        Err(Stop::NoMatch)
    }

    /// Mark the innermost choice or repetition entry of the rule activation. Entries that are
    /// no choice points are passed, and a call or a lookahead ends the search, since a cut
    /// reaches neither the caller of its rule nor past the lookahead it stands in.
    fn cut(&mut self) {
        let mut passed = 0;
        for entry in self.stack.iter_mut().rev() {
            match entry {
                Entry::Choice { cut, .. } | Entry::Repetition { cut, .. } => {
                    *cut = true;
                    break;
                }
                Entry::Call { .. } | Entry::Frame | Entry::Predicate { .. } => break,
                Entry::Label { .. }
                | Entry::Sub { .. }
                | Entry::Open { .. }
                | Entry::Mark { .. }
                | Entry::Flags { .. } => passed += 1,
            }
        }
        self.steps += passed;
    }

    /// The open token failed when the entry that caught the failure, at `catcher` on the stack,
    /// is older than the token, or when nothing caught it. Its start is recorded then, with the
    /// quiet count of the failure.
    fn end_failed_token(&mut self, catcher: Option<usize>) {
        if let Some(token) = self.token
            && catcher.is_none_or(|at| (at as u32) < token.height)
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
                Entry::Predicate { alt, saved, quiet } => {
                    if self
                        .token
                        .is_some_and(|token| (self.stack.len() as u32) < token.height)
                    {
                        self.token = None;
                    }
                    self.restore(saved);
                    self.quiet = quiet;
                    self.pc = alt;
                    return Ok(());
                }
                Entry::Call { .. } => self.depth -= 1,
                // An error is no result to remember.
                Entry::Frame => {
                    self.drop_frame();
                }
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

    /// How many entries the tree of the parse has. A remembered match can be given to several
    /// parents, and the tree repeats such a node under each, so the tree can be far larger than
    /// the arena. Children lie before their parents in the arena, so one pass sizes every node.
    fn tree_entries(&self) -> u64 {
        let mut sizes: Vec<u64> = Vec::with_capacity(self.nodes.len());
        let size = |sizes: &[u64], item: &Item| match item.flags & TOKEN {
            0 => sizes.get(item.value as usize).copied().unwrap_or(0),
            _ => 1,
        };
        for node in &self.nodes {
            let children = (node.first as usize..(node.first + node.count) as usize)
                .filter_map(|index| self.children.get(index));
            let entries = children.fold(1u64, |sum, child| sum.saturating_add(size(&sizes, child)));
            sizes.push(entries);
        }
        let roots = self.items.iter();
        roots.fold(0, |sum, item| sum.saturating_add(size(&sizes, item)))
    }

    /// Lay out the items of a successful parse as the top level of a tree, in postorder. Each
    /// entry counts as a step, so a tree that repeats shared nodes beyond the steps the parse
    /// has left is refused before any of it is built.
    pub fn tree(&self) -> Result<Tree, Limit> {
        if self.shared {
            let left = self.limits.max_steps.saturating_sub(self.steps);
            if self.tree_entries() > left.min(u64::from(u32::MAX)) {
                return Err(Limit::Steps);
            }
        }
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
        Ok(tree)
    }
}
