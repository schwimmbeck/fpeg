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
//! recursion for its rule, position, flags and context, a call that finds it takes the match of the
//! pass before, its seed, and the rule's routine runs again for as long as each pass ends
//! further.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{BuildHasher, DefaultHasher, Hash, Hasher, RandomState};
use std::sync::Arc;

use super::profile::{Profile, Profiler};
use super::program::{Inst, Program};
use crate::parse::{END_OF_INPUT, Found, INDENTATION, Limit, Limits, Recovery};
use crate::tree::{FieldId, KindId, Tree};

/// Bytes of a literal compared in one step, about the work of one instruction.
const LITERAL_BYTES_PER_STEP: u64 = 8;

/// Bytes before a position for each step that finding its column counts. It reads them
/// twice, back to the start of the line and forward, with more work for each than a literal.
const COLUMN_BYTES_PER_STEP: u64 = 3;

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
    /// The reference column before an `Indent`.
    Column {
        column: u32,
    },
    /// Where the text of a capture starts.
    Def {
        start: u32,
    },
    /// How many captures there were, and the context, at a `CaptureMark`.
    Captures {
        height: u32,
        context: u32,
    },
    /// A call of a rule that is remembered or on a left cycle. What it needs is the newest of
    /// [`Machine::frames`], so that the entries keep their size.
    Frame,
    /// A loop of left recursion, whose state is the newest of [`Machine::loops`].
    Loop,
    /// A recovery in progress, whose state is the newest of [`Machine::recovering_frames`].
    Recovering,
}

/// Where the parse had got furthest under a floor, and what had failed there.
#[derive(Clone, Debug)]
struct Furthest {
    at: u32,
    expected: BTreeSet<Expected>,
}

/// An error alternative matched outside a lookahead: which, where, and the context there, so
/// that it can be tested once the parse has failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Attempt {
    index: u32,
    pos: u32,
    flags: u64,
    column: u32,
    /// The id of the visible captures.
    map: u32,
    floor: Option<u32>,
}

/// A recovery in progress: where it returns to, its label and the field of what failed,
/// where it started, the height of the items then, and the floor and context it replaced.
#[derive(Clone, Copy, Debug)]
struct RecoveryFrame {
    ret: u32,
    label: u32,
    field: FieldId,
    start: u32,
    items: u32,
    floor: Option<u32>,
    context: u32,
}

/// A loop of left recursion in progress: the height of the items when it started, and its
/// seed, the match of the pass before with its items in [`Machine::seeds`].
#[derive(Clone, Copy, Debug)]
struct Seeded {
    items: u32,
    seed: Option<Match>,
}

/// What a remembered result depends on: the rule, the position, the flags that are on, the rest
/// of the context, whether failures were being recorded, since a result found while they were
/// not recorded none, and whether the call ran where error alternatives record no attempts,
/// inside a lookahead of a run that explains a failed parse. Without the last two, it is the
/// key of the table of left recursion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MemoKey {
    rule: u32,
    pos: u32,
    flags: u64,
    /// The context besides the flags: the reference column and the visible captures, as an id
    /// of the parse.
    context: u32,
    quiet: bool,
    looking: bool,
}

impl Hash for MemoKey {
    /// Three words, one multiplication each under the memo's hasher.
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(u64::from(self.rule) << 32 | u64::from(self.pos));
        state.write_u64(self.flags);
        let states = u64::from(self.quiet) << 1 | u64::from(self.looking);
        state.write_u64(u64::from(self.context) << 2 | states);
    }
}

/// Makes the hashers of the memo, by one multiplication a word with keys drawn for each parse
/// or, with the pass off, by SipHash.
#[derive(Clone)]
enum KeyState {
    Multiply { seed: u64, key: u64 },
    Sip(RandomState),
}

impl KeyState {
    /// A state with keys of its own, taken from those of SipHash, multiplying where `multiply`.
    fn new(multiply: bool) -> KeyState {
        let sip = RandomState::new();
        match multiply {
            true => KeyState::Multiply {
                seed: sip.hash_one(0u8),
                key: sip.hash_one(1u8),
            },
            false => KeyState::Sip(sip),
        }
    }
}

impl BuildHasher for KeyState {
    type Hasher = KeyHasher;

    fn build_hasher(&self) -> KeyHasher {
        match *self {
            KeyState::Multiply { seed, key } => KeyHasher::Multiply { hash: seed, key },
            KeyState::Sip(ref state) => KeyHasher::Sip(state.build_hasher()),
        }
    }
}

/// Hashes a key of the memo. Each word is multiplied by the key to a product of two words,
/// whose high half is folded onto its low half, so that every bit of the word reaches the bits
/// that pick a bucket. The keys are drawn for each parse: whoever writes a grammar and its input
/// could otherwise choose calls whose keys share buckets, and each lookup would then scan a part
/// of the table that grows with the input.
enum KeyHasher {
    Multiply { hash: u64, key: u64 },
    Sip(DefaultHasher),
}

impl KeyHasher {
    fn add(&mut self, word: u64) {
        match self {
            KeyHasher::Multiply { hash, key } => {
                let product = u128::from(*hash ^ word) * u128::from(*key);
                // Truncation keeps the low half, which the high half is folded onto.
                *hash = product as u64 ^ (product >> 64) as u64;
            }
            KeyHasher::Sip(state) => state.write_u64(word),
        }
    }
}

impl Hasher for KeyHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, n: u8) {
        self.add(u64::from(n));
    }

    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    fn finish(&self) -> u64 {
        match self {
            KeyHasher::Multiply { hash, .. } => *hash,
            KeyHasher::Sip(state) => state.finish(),
        }
    }
}

/// The hashes that the memo of one parse gives keys of a rule, a position and flags.
#[cfg(test)]
pub fn memo_hashes(keys: impl Iterator<Item = (u32, u32, u64)>) -> Vec<u64> {
    let state = KeyState::new(true);
    keys.map(|(rule, pos, flags)| {
        state.hash_one(MemoKey {
            rule,
            pos,
            flags,
            context: 0,
            quiet: false,
            looking: false,
        })
    })
    .collect()
}

/// The id after `count` ids from 1. Past 32 bits two contexts, texts or maps would share an
/// id and so a key, so the parse stops as at its memory limit, which it would reach first at
/// the default limits.
fn id_after(count: usize) -> Result<u32, Stop> {
    (u32::try_from(count).ok())
        .and_then(|count| count.checked_add(1))
        .ok_or(Stop::Limit(Limit::Memory))
}

/// A skip that ran the skip routine: where it started, with which flags and context, and where
/// it ended. Skipping depends on nothing else, since the skip expression makes no items, records
/// no failures and calls no rule that skips, so a skip from there with those flags and that
/// context ends there. The context counts only where the skip expression reads it. The floor
/// counts, since the attempts of error alternatives count under it, and a run that explains a
/// failed parse also tells apart skips inside a lookahead, where they record none.
#[derive(Clone, Copy, Debug)]
struct SkipCache {
    state: SkipState,
    end: u32,
}

/// What the end of a skip depends on: where it starts, the flags, the context that the skip
/// expression reads, the floor, and whether it runs where attempts are not recorded.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SkipState {
    start: u32,
    flags: u64,
    context: u32,
    floor: Option<u32>,
    looking: bool,
}

/// The room a vector of the machine takes when it first grows, in elements.
const FIRST_ROOM: usize = 16;

/// The buckets a table takes when it first grows, as the standard library's does.
const FIRST_BUCKETS: u64 = 4;

/// The buckets of a table with room for `capacity` entries: a power of two, of which the table
/// keeps an eighth empty, as the standard library's does.
fn buckets(capacity: usize) -> u64 {
    match capacity {
        0 => 0,
        _ => (capacity as u64 * 8 / 7).next_power_of_two(),
    }
}

/// The memory a parse holds and may hold, in bytes: the room of the machine's vectors and
/// tables and of a profiler's, which never shrink during a parse.
#[derive(Clone, Copy, Debug)]
pub(super) struct Memory {
    held: u64,
    limit: u64,
}

impl Memory {
    /// Nothing held yet, and at most `limit` bytes.
    pub(super) fn new(limit: u64) -> Memory {
        Memory { held: 0, limit }
    }

    /// The bytes held.
    #[cfg(test)]
    pub fn held(&self) -> u64 {
        self.held
    }

    /// Make room for `more` elements in `vec` as it would grow by itself, doubling, unless the
    /// room it gains takes the parse past its limit.
    pub(super) fn room<T>(&mut self, vec: &mut Vec<T>, more: usize) -> Result<(), Stop> {
        if vec.capacity() - vec.len() >= more {
            return Ok(());
        }
        self.grow(vec, more)
    }

    /// Grow `vec` as [`Memory::room`] says. While a block moves, the old one and the new one
    /// are both held, so the new one counts in full before the old one is given back.
    #[cold]
    fn grow<T>(&mut self, vec: &mut Vec<T>, more: usize) -> Result<(), Stop> {
        let before = vec.capacity();
        let wanted = (vec.len() + more).max(before * 2).max(FIRST_ROOM);
        self.take((wanted * size_of::<T>()) as u64)?;
        vec.reserve_exact(wanted - vec.len());
        // The allocator may give more room than asked for, which counts too.
        self.held += ((vec.capacity() - wanted) * size_of::<T>()) as u64;
        self.held = self.held.saturating_sub((before * size_of::<T>()) as u64);
        Ok(())
    }

    /// Make room for one more entry in a table, which moves its entries to twice the buckets,
    /// each of an entry and a byte of control, when it is full, and gives the old ones back.
    pub(super) fn room_in<K: Hash + Eq, V, S: BuildHasher>(
        &mut self,
        table: &mut HashMap<K, V, S>,
    ) -> Result<(), Stop> {
        if table.len() < table.capacity() {
            return Ok(());
        }
        let bucket = (size_of::<(K, V)>() + 1) as u64;
        let before = buckets(table.capacity());
        self.take((before * 2).max(FIRST_BUCKETS) * bucket)?;
        table.reserve(1);
        self.held = self.held.saturating_sub(before * bucket);
        Ok(())
    }

    /// Count `bytes` more, unless they take the parse past its limit.
    fn take(&mut self, bytes: u64) -> Result<(), Stop> {
        match self.held.saturating_add(bytes) {
            held if held <= self.limit => {
                self.held = held;
                Ok(())
            }
            _ => Err(Stop::Limit(Limit::Memory)),
        }
    }
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

/// The steps that an entry repeating a shared node counts as when the tree is laid out: what
/// the cheapest entry, a literal in a sequence, costs a parse that shares nothing, so that
/// sharing builds no larger tree than the same steps could without it.
const TREE_ENTRY_STEPS: u64 = 4;

/// No frame, where an index of [`Machine::frames`] is expected.
const NO_FRAME: u32 = u32::MAX;

/// Whether an item is a node that holds an error item, as `holds` says for each node.
fn holding(holds: &[bool], item: &Item) -> bool {
    item.flags & TOKEN == 0 && holds.get(item.value as usize).copied().unwrap_or(false)
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
    /// A labeled error, with the index of its label, its position and the floor it failed
    /// under.
    Error {
        label: u32,
        position: u32,
        floor: Option<u32>,
    },
    Limit(Limit),
    /// The steps of the run's budget ran out before those of the limit.
    Budget,
}

/// What a failure expects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Expected {
    Kind(KindId),
    EndOfInput,
    /// A column check failed.
    Indentation,
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
    /// The items of the seeds of the frames and loops of left recursion, the newest last.
    seeds: Vec<Item>,
    /// The loops of left recursion in progress, one for each `Loop` entry of the stack.
    loops: Vec<Seeded>,
    /// The newest frame of left recursion, or [`NO_FRAME`].
    newest_recursion: u32,
    /// The serial of the next frame of left recursion.
    serial: u64,
    /// The oldest frame of left recursion whose entry the evaluation in progress used, or
    /// [`NO_FRAME`]. A result that used a frame older than its own call is not remembered.
    dependency: u32,
    memo: HashMap<MemoKey, Stored, KeyState>,
    /// The items of the remembered matches.
    memo_items: Vec<Item>,
    /// The length of the arena at the newest remembered match that holds a node. The nodes
    /// below it stay, since remembered items point at them.
    protected: u32,
    /// Whether a remembered match or an empty seed was given again, so that two parents may
    /// hold one node.
    shared: bool,
    /// The last skip that ran the skip routine.
    skip_cache: Option<SkipCache>,
    /// Where the skip routine in progress started.
    skip_start: u32,
    /// Where the skip routine returns, or `u32::MAX`.
    skip_ret: u32,
    token: Option<OpenToken>,
    flags: u64,
    /// The reference column of `%indent` and `%same`.
    column: u32,
    /// For each text remembered under a capture, oldest first, the id of the captures visible
    /// with it: those of the activation in progress above those of its callers, so that the
    /// last holds the newest text of each capture.
    captures: Vec<u32>,
    /// The id of the reference column, the visible captures and the floor, 0 for column 0,
    /// none and none.
    context: u32,
    /// The ids of the contexts met, by column, the id of the visible captures and the floor.
    contexts: HashMap<(u32, u32, Option<u32>), u32>,
    /// Whether the parse recovers from labeled errors.
    recovering: bool,
    /// Inside a recovery, where it started: a label recovers only past it.
    floor: Option<u32>,
    /// The recoveries in progress, one for each `Recovering` entry of the stack.
    recovering_frames: Vec<RecoveryFrame>,
    /// The label of each error item made and the floor it failed under, by the arena index of
    /// its node. An index that a failure gave back is taken again by the next node, so an entry
    /// is current where its node is of kind `<error>` or `<missing>`.
    labels: HashMap<u32, (u32, Option<u32>)>,
    /// What a parse that stopped without a tree recovered from on its way.
    ended: Vec<Found>,
    /// Whether the run records the attempts of error alternatives, to explain a failed parse.
    explaining: bool,
    /// The attempts of error alternatives made outside lookaheads, each once.
    attempts: HashMap<Attempt, ()>,
    /// How many lookaheads enclose the code in progress.
    looking: u32,
    /// The ids of the captured texts met, one for equal texts, and the texts by id.
    texts: HashMap<&'i str, u32>,
    text_list: Vec<&'i str>,
    /// The visible captures met, each the text id of each capture sorted by capture, and their
    /// ids, from 1, since 0 is none. `map_list` holds the one of id `n` at `n - 1`.
    maps: HashMap<Box<[(u16, u32)]>, u32>,
    map_list: Vec<Box<[(u16, u32)]>>,
    /// Failures are recorded only while this is 0: outside lookahead and the skip routine.
    quiet: u32,
    depth: u32,
    steps: u64,
    limits: Limits,
    /// The steps the run may take, at most those of the limit.
    budget: u64,
    memory: Memory,
    /// The farthest position a failure was recorded at.
    farthest: Option<u32>,
    /// In diagnose mode, the positions the run before asks about, each with the floor of the
    /// error there and what failed at it under that floor.
    diagnose: BTreeMap<(u32, Option<u32>), BTreeSet<Expected>>,
    /// In a run that explains recoveries, where the parse got furthest under each floor.
    furthest: Option<HashMap<Option<u32>, Furthest>>,
    /// In a run that explains recoveries, the error each one explains, by its label, position
    /// and floor: where the parse had got furthest under the floor when the label first failed
    /// there.
    explanations: HashMap<(u32, u32, Option<u32>), Furthest>,
    /// The counts of a profiling parse.
    profiler: Option<Box<Profiler>>,
}

impl<'p, 'i> Machine<'p, 'i> {
    /// A machine for one parse. The input must fit 32-bit offsets. It collects what fails at the
    /// positions of `diagnose` under their floors, which a run after the first asks about.
    pub fn new(
        program: &'p Program,
        input: &'i str,
        limits: Limits,
        diagnose: &[(u32, Option<u32>)],
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
            loops: Vec::new(),
            newest_recursion: NO_FRAME,
            serial: 0,
            dependency: NO_FRAME,
            memo: HashMap::with_hasher(KeyState::new(program.passes.hasher)),
            memo_items: Vec::new(),
            protected: 0,
            shared: false,
            skip_cache: None,
            skip_start: 0,
            skip_ret: program.skip_return().unwrap_or(u32::MAX),
            token: None,
            flags: 0,
            column: 0,
            captures: Vec::new(),
            context: 0,
            contexts: HashMap::new(),
            recovering: false,
            floor: None,
            recovering_frames: Vec::new(),
            labels: HashMap::new(),
            ended: Vec::new(),
            explaining: false,
            attempts: HashMap::new(),
            looking: 0,
            texts: HashMap::new(),
            text_list: Vec::new(),
            maps: HashMap::new(),
            map_list: Vec::new(),
            quiet: 0,
            depth: 0,
            steps: 0,
            limits,
            budget: limits.max_steps,
            memory: Memory::new(limits.memory_bytes(input.len())),
            farthest: None,
            diagnose: (diagnose.iter()).map(|&at| (at, BTreeSet::new())).collect(),
            furthest: None,
            explanations: HashMap::new(),
            profiler: None,
        }
    }

    /// Count what each rule does, for [`Machine::profile`].
    pub fn profiling(mut self) -> Machine<'p, 'i> {
        self.profiler = Some(Box::new(Profiler::new(self.program)));
        self
    }

    /// The machine for a parse in recovering mode.
    pub fn recovering(mut self) -> Machine<'p, 'i> {
        self.recovering = true;
        self
    }

    /// The machine for a run that records the attempts of error alternatives, to explain a
    /// failed parse.
    pub fn explaining(mut self) -> Machine<'p, 'i> {
        self.explaining = true;
        self
    }

    /// The error alternative that explains the failed parse whose error stands under `floor`.
    /// The attempts under it are tested in the order of their positions, and at one position
    /// in the order of the text, each as a parse of its own, with the steps the run
    /// left. The index of the first that matches, and where its longest match starts and ends,
    /// or none where none matches, or the limit the tests reach. Where it was tried in several
    /// contexts, the farthest end over them, and the farthest start after the ignored text over
    /// those that reach it, so that the order of the contexts counts for nothing.
    pub fn explain(&self, floor: Option<u32>) -> Result<Option<(u32, u32, u32)>, Stop> {
        let mut attempts: Vec<Attempt> = (self.attempts.keys())
            .filter(|attempt| attempt.floor == floor)
            .copied()
            .collect();
        let rank = |a: &Attempt| self.program.error_ranks.get(a.index as usize).copied();
        // The attempts are distinct, and the key holds all of each but the floor they share.
        attempts.sort_by_key(|a| (a.pos, rank(a), a.index, a.flags, a.column, a.map));
        // The tests take what the run left, so that the parse holds no more than its limits.
        let left = Limits {
            max_steps: self.limits.max_steps.saturating_sub(self.steps),
            max_memory_bytes: Some(self.memory.limit.saturating_sub(self.memory.held)),
            ..self.limits
        };
        let mut tester = Machine::new(self.program, self.input, left, &[]);
        // The longest match of the first alternative that matches, and the contexts it reaches
        // that end in.
        let mut found: Option<(u32, Vec<Attempt>)> = None;
        for attempt in attempts {
            if let Some((_, reaching)) = &found
                && (reaching[0].pos, reaching[0].index) != (attempt.pos, attempt.index)
            {
                break;
            }
            let Some(&Some(routine)) = self.program.error_tests.get(attempt.index as usize) else {
                continue;
            };
            let captures = self.captured(attempt.map);
            match (
                tester.test_attempt(routine, &attempt, &captures)?,
                &mut found,
            ) {
                (Some(end), Some((longest, reaching))) if end == *longest => reaching.push(attempt),
                (Some(end), found) if found.as_ref().is_none_or(|(longest, _)| end > *longest) => {
                    *found = Some((end, vec![attempt]));
                }
                _ => {}
            }
        }
        let Some((end, reaching)) = found else {
            return Ok(None);
        };
        // The error starts after the ignored text, as a label's does, or where the attempt was
        // made where the ignored text there does not match.
        let index = reaching[0].index;
        let mut start = reaching[0].pos;
        if self.program.error_skips.get(index as usize) == Some(&true)
            && let Some(routine) = self.program.skip_test
        {
            for attempt in &reaching {
                let captures = self.captured(attempt.map);
                if let Some(skipped) = tester.test_attempt(routine, attempt, &captures)? {
                    start = start.max(skipped);
                }
            }
        }
        Ok(Some((index, start, end.max(start))))
    }

    /// Whether error alternatives record no attempts where they could: inside a lookahead of a
    /// run that explains a failed parse. What is remembered or cached there is kept apart, so
    /// that a later match outside the lookahead makes its attempts. Other runs record none at
    /// all and keep nothing apart.
    fn unattempted(&self) -> bool {
        self.explaining && self.looking > 0
    }

    /// The visible captures with this id, each by name with its text.
    fn captured(&self, map: u32) -> Vec<(u16, &'i str)> {
        let Some(map) = (map as usize)
            .checked_sub(1)
            .and_then(|i| self.map_list.get(i))
        else {
            return Vec::new();
        };
        (map.iter())
            .filter_map(|&(name, text)| Some((name, *self.text_list.get(text as usize)?)))
            .collect()
    }

    /// Match the test routine of an error alternative at an attempt, sheltered, as a parse of
    /// its own: where it ends if it matches. What earlier tests remembered stays valid, since a
    /// result depends on its key alone.
    fn test_attempt(
        &mut self,
        routine: u32,
        attempt: &Attempt,
        captures: &[(u16, &'i str)],
    ) -> Result<Option<u32>, Stop> {
        self.stack.clear();
        self.items.clear();
        self.recovering_frames.clear();
        self.token = None;
        self.depth = 0;
        self.quiet = 1;
        self.looking = 1;
        self.floor = None;
        self.pos = attempt.pos;
        self.flags = attempt.flags;
        self.column = attempt.column;
        self.captures.clear();
        for &(name, text) in captures {
            self.remember(name, text)?;
        }
        self.context = self.context_of(self.column, self.map())?;
        match self.run(routine) {
            Ok(()) => Ok(Some(self.pos)),
            Err(Stop::NoMatch | Stop::Error { .. }) => Ok(None),
            Err(stop) => Err(stop),
        }
    }

    /// What a parse that stopped without a tree recovered from on its way, in the order of
    /// the text.
    pub fn take_recoveries(&mut self) -> Vec<Found> {
        std::mem::take(&mut self.ended)
    }

    /// The machine that stops with [`Stop::Budget`] after `budget` steps, where they are
    /// fewer than the limit allows.
    pub fn budgeted(mut self, budget: u64) -> Machine<'p, 'i> {
        self.budget = budget.min(self.limits.max_steps);
        self
    }

    /// The steps the parse took.
    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// What the parse did, if the machine was profiling.
    pub fn profile(&mut self) -> Option<Profile> {
        let profiler = self.profiler.take()?;
        Some(profiler.finish(self.steps, self.nodes.len(), self.memo.len()))
    }

    /// Tell a profiler that the routine of the newest call is over, failed or not.
    fn profile_left(&mut self, failed: bool) {
        if let Some(profiler) = self.profiler.as_deref_mut() {
            profiler.left(self.steps, failed.then_some(self.pos));
        }
    }

    /// The farthest position a failure was recorded at outside recoveries.
    pub fn farthest(&self) -> Option<u32> {
        self.farthest
    }

    /// How many nodes the arena holds.
    #[cfg(test)]
    pub fn arena_len(&self) -> usize {
        self.nodes.len()
    }

    /// The machine for a run that also explains the recoveries it makes.
    pub fn explaining_recoveries(mut self) -> Machine<'p, 'i> {
        self.furthest = Some(HashMap::new());
        self
    }

    /// In a run that explains recoveries, the error the recovery of a label at a position under
    /// a floor explains: where the parse had got furthest under the floor when the label first
    /// failed there, and what had failed there by then, as an expected set shows it, sorted.
    pub fn explanation(&self, label: u32, start: u32, floor: Option<u32>) -> (u32, Vec<String>) {
        match self.explanations.get(&(label, start, floor)) {
            Some(furthest) => (furthest.at, self.shown(&furthest.expected)),
            None => (start, Vec::new()),
        }
    }

    /// In diagnose mode, what failed at a position it was asked about under a floor, as an
    /// expected set shows it, sorted.
    pub fn expected(&self, at: u32, floor: Option<u32>) -> Vec<String> {
        match self.diagnose.get(&(at, floor)) {
            Some(expected) => self.shown(expected),
            None => Vec::new(),
        }
    }

    /// What failed, as an expected set shows it, sorted.
    fn shown(&self, expected: &BTreeSet<Expected>) -> Vec<String> {
        let shown: BTreeSet<String> = (expected.iter())
            .map(|what| match what {
                Expected::Kind(kind) => (self.program.kind_display)
                    .get(kind.0 as usize)
                    .cloned()
                    .unwrap_or_default(),
                Expected::EndOfInput => END_OF_INPUT.to_owned(),
                Expected::Indentation => INDENTATION.to_owned(),
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
            if self.steps > self.budget {
                return Err(match self.budget < self.limits.max_steps {
                    true => Stop::Budget,
                    false => Stop::Limit(Limit::Steps),
                });
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
                Inst::Span { class, min } => {
                    let (len, count) = self.run_of(class);
                    self.advance((count >= min).then_some(len))?;
                }
                Inst::Test { test, target } => {
                    self.pc = if self.next_byte_fits(test) {
                        self.pc + 1
                    } else {
                        target
                    };
                }
                Inst::TestFail(test) => {
                    let fits = self.next_byte_fits(test);
                    self.test(fits)?;
                }
                Inst::SeedOpen => {
                    self.entry(Entry::Loop)?;
                    self.memory.room(&mut self.loops, 1)?;
                    self.loops.push(Seeded {
                        items: self.items.len() as u32,
                        seed: None,
                    });
                    self.pc += 1;
                }
                Inst::SeedKeep => {
                    self.keep_seed()?;
                    self.pc += 1;
                }
                Inst::SeedUse => {
                    let seed = self.loops.last().and_then(|state| state.seed);
                    if let Some(seed) = seed {
                        let items = self.seeds.get(seed.range()).unwrap_or_default();
                        self.steps += items.len() as u64;
                        self.memory.room(&mut self.items, items.len())?;
                        self.items.extend_from_slice(items);
                    }
                    self.pc += 1;
                }
                Inst::SeedGrow(target) => match self.stack.pop() {
                    Some(Entry::Choice { saved, .. }) if self.pos > saved.pos => {
                        if let Some(profiler) = self.profiler.as_deref_mut() {
                            profiler.loop_grew();
                        }
                        self.pc = target;
                    }
                    Some(Entry::Choice { alt, saved, .. }) => {
                        self.restore(saved);
                        self.pc = alt;
                    }
                    _ => return Err(Stop::NoMatch),
                },
                Inst::SeedClose => {
                    self.stack.pop();
                    if let Some(seed) = self.loops.pop().and_then(|state| state.seed) {
                        let items = self.seeds.get(seed.range()).unwrap_or_default();
                        self.memory.room(&mut self.items, items.len())?;
                        self.items.extend_from_slice(items);
                        self.seeds.truncate(seed.first as usize);
                    }
                    self.pc += 1;
                }
                Inst::Choice(alt) => self.push(Entry::Choice {
                    alt,
                    saved: self.saved(),
                    cut: false,
                })?,
                Inst::LabelChoice(alt) => self.push(Entry::Label {
                    alt,
                    saved: self.saved(),
                })?,
                Inst::PredChoice(alt) => {
                    self.push(Entry::Predicate {
                        alt,
                        saved: self.saved(),
                        quiet: self.quiet,
                    })?;
                    self.quiet += 1;
                    self.looking += 1;
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
                    })?;
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
                        self.looking -= 1;
                    }
                    self.pc = target;
                }
                Inst::FailTwice => {
                    if let Some(Entry::Predicate { quiet, .. }) = self.stack.pop() {
                        self.quiet = quiet;
                        self.looking -= 1;
                    }
                    self.fail()?;
                }
                Inst::ErrorAttempt(index) => {
                    if self.explaining && self.looking == 0 {
                        self.memory.room_in(&mut self.attempts)?;
                        let attempt = Attempt {
                            index,
                            pos: self.pos,
                            flags: self.flags,
                            column: self.column,
                            map: self.map(),
                            floor: self.floor,
                        };
                        self.attempts.insert(attempt, ());
                    }
                    self.fail()?;
                }
                Inst::TestEnd => return Ok(()),
                Inst::Throw(label) => self.throw(label)?,
                Inst::Recover { label, field } => self.recover(label, field)?,
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
                    let context = self.context;
                    let (quiet, looking) = (self.quiet > 0, self.unattempted());
                    if let Some(profiler) = self.profiler.as_deref_mut() {
                        let rule = self.program.rule_at(target);
                        let key = (rule, self.pos, self.flags, context, quiet, looking);
                        profiler.called(key, false, &mut self.memory)?;
                        let (stack, nodes) = (self.stack.len(), self.nodes.len());
                        profiler.entered(rule, self.pos, self.steps, stack, nodes);
                    }
                    self.entry(Entry::Call { ret: self.pc + 1 })?;
                    self.pc = target;
                }
                Inst::MemoCall(rule) => self.call_frame(rule, false)?,
                Inst::LrCall(rule) => self.call_frame(rule, true)?,
                Inst::Sub(target) => {
                    self.entry(Entry::Sub { ret: self.pc + 1 })?;
                    self.pc = target;
                }
                Inst::Skip => match (self.program.skip_class, self.program.skip) {
                    (Some(class), _) => {
                        let (len, _) = self.run_of(class);
                        self.advance(Some(len))?;
                    }
                    (None, Some(_)) if self.skips_nothing() => self.pc += 1,
                    (None, Some(_))
                        if self.program.passes.skip
                            && let Some(end) = self.cached_skip_end() =>
                    {
                        self.pos = end;
                        self.pc += 1;
                    }
                    (None, Some(skip)) => {
                        if let Some(profiler) = self.profiler.as_deref_mut() {
                            profiler.skip(self.steps);
                        }
                        self.skip_start = self.pos;
                        self.entry(Entry::Sub { ret: self.pc + 1 })?;
                        self.pc = skip;
                    }
                    (None, None) => self.pc += 1,
                },
                Inst::Ret => match self.stack.pop() {
                    Some(Entry::Call { ret }) => {
                        self.depth -= 1;
                        self.profile_left(false);
                        self.pc = ret;
                    }
                    Some(Entry::Sub { ret }) => {
                        if let Some(profiler) = self.profiler.as_deref_mut() {
                            profiler.returned_from(self.pc, self.steps);
                        }
                        if self.pc == self.skip_ret {
                            self.skip_cache = Some(SkipCache {
                                state: self.skip_state(self.skip_start),
                                end: self.pos,
                            });
                        }
                        self.pc = ret;
                    }
                    Some(Entry::Frame) => self.return_frame()?,
                    Some(Entry::Recovering) => self.recovered()?,
                    _ => return Err(Stop::NoMatch),
                },
                Inst::End => {
                    if self.pos as usize == self.input.len() {
                        return Ok(());
                    }
                    self.record(self.pos, Expected::EndOfInput);
                    if self.recovering {
                        self.ended = self.recoveries_among()?;
                    }
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
                        self.memory.room(&mut self.items, 1)?;
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
                })?,
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
                    self.entry(Entry::Mark { pos: self.pos })?;
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
                    self.entry(Entry::Flags { saved: self.flags })?;
                    self.flags |= 1 << flag;
                    self.pc += 1;
                }
                Inst::FlagOff(flag) => {
                    self.entry(Entry::Flags { saved: self.flags })?;
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
                Inst::Indent { skip } => self.indent(skip)?,
                Inst::IndentEnd => {
                    if let Some(Entry::Column { column }) = self.stack.pop() {
                        self.end_column(column)?;
                    }
                    self.pc += 1;
                }
                Inst::Same { skip } => {
                    if self.column_at(self.pos) == self.column {
                        self.take_back(skip);
                        self.pc += 1;
                    } else {
                        self.record(self.pos, Expected::Indentation);
                        self.fail()?;
                    }
                }
                Inst::AtEnd { skip } => {
                    if self.pos as usize == self.input.len() {
                        self.take_back(skip);
                        self.pc += 1;
                    } else {
                        self.record(self.pos, Expected::EndOfInput);
                        self.fail()?;
                    }
                }
                Inst::DefOpen { skip } => {
                    let start = self.pos;
                    self.take_back(skip);
                    self.push(Entry::Def { start })?;
                }
                Inst::DefClose(name) => {
                    if let Some(Entry::Def { start }) = self.stack.pop() {
                        self.capture(name, start.min(self.pos))?;
                    }
                    self.pc += 1;
                }
                Inst::Ref(name) => {
                    let len = self.visible(name).and_then(|text| {
                        // Comparing a text is work of its length, as for a literal.
                        self.steps += text.len() as u64 / LITERAL_BYTES_PER_STEP;
                        self.rest().starts_with(text).then_some(text.len())
                    });
                    self.advance(len)?;
                }
                Inst::CaptureMark => {
                    let height = self.captures.len() as u32;
                    let context = self.context;
                    self.push(Entry::Captures { height, context })?;
                }
                Inst::CaptureKeep => {
                    self.stack.pop();
                    self.pc += 1;
                }
                Inst::CaptureDrop => {
                    if let Some(Entry::Captures { height, context }) = self.stack.pop() {
                        self.give_back(height, context);
                    }
                    self.pc += 1;
                }
            }
        }
    }

    /// Check that the column of the position is right of the reference column and make it the
    /// reference column, or record the failed check and fail.
    fn indent(&mut self, skip: bool) -> Result<(), Stop> {
        let column = self.column_at(self.pos);
        if column <= self.column {
            self.record(self.pos, Expected::Indentation);
            return self.fail();
        }
        self.take_back(skip);
        self.entry(Entry::Column {
            column: self.column,
        })?;
        self.column = column;
        self.context = self.context_of(column, self.map())?;
        self.pc += 1;
        Ok(())
    }

    /// Restore the reference column of before an `Indent`. A capture made inside the block
    /// outlives it, so the context is that of the column with the captures visible now.
    fn end_column(&mut self, column: u32) -> Result<(), Stop> {
        self.column = column;
        self.context = self.context_of(column, self.map())?;
        Ok(())
    }

    /// Give back the captures made since a `CaptureMark`, with the context of then. The column
    /// is that of then too, since every `Indent` since has been left.
    fn give_back(&mut self, height: u32, context: u32) {
        self.captures.truncate(height as usize);
        self.context = context;
    }

    /// The id of the visible captures.
    fn map(&self) -> u32 {
        self.captures.last().copied().unwrap_or(0)
    }

    /// The text visible under a capture, found in the map of the visible captures. A walk
    /// down the stack would pass every capture kept since, also of other names.
    fn visible(&self, name: u16) -> Option<&'i str> {
        let map = self.map_list.get((self.map() as usize).checked_sub(1)?)?;
        let at = map
            .binary_search_by_key(&name, |&(capture, _)| capture)
            .ok()?;
        self.text_list.get(map[at].1 as usize).copied()
    }

    /// The id of the context of a column and visible captures with the floor, made when it is
    /// new.
    fn context_of(&mut self, column: u32, map: u32) -> Result<u32, Stop> {
        let key = (column, map, self.floor);
        if key == (0, 0, None) {
            return Ok(0);
        }
        if let Some(&id) = self.contexts.get(&key) {
            return Ok(id);
        }
        self.memory.room_in(&mut self.contexts)?;
        let id = id_after(self.contexts.len())?;
        self.contexts.insert(key, id);
        Ok(id)
    }

    /// Remember the text from `start` to the position under the capture `name`: its text id, the
    /// visible captures with it, and their context. Hashing the text and sorting the captures is
    /// work of their sizes.
    fn capture(&mut self, name: u16, start: u32) -> Result<(), Stop> {
        let input = self.input;
        let text = input
            .get(start as usize..self.pos as usize)
            .unwrap_or_default();
        self.remember(name, text)
    }

    /// Remember `text` under the capture `name`, as `capture` does.
    fn remember(&mut self, name: u16, text: &'i str) -> Result<(), Stop> {
        self.steps += text.len() as u64 / LITERAL_BYTES_PER_STEP + 1;
        let text_id = match self.texts.get(text) {
            Some(&id) => id,
            None => {
                self.memory.room_in(&mut self.texts)?;
                self.memory.room(&mut self.text_list, 1)?;
                let id = id_after(self.text_list.len())? - 1;
                self.texts.insert(text, id);
                self.text_list.push(text);
                id
            }
        };
        let old = (self.map() as usize).checked_sub(1);
        let mut map = old
            .and_then(|i| self.map_list.get(i))
            .map_or_else(Vec::new, |m| m.to_vec());
        match map.binary_search_by_key(&name, |&(capture, _)| capture) {
            Ok(at) => map[at].1 = text_id,
            Err(at) => map.insert(at, (name, text_id)),
        }
        self.steps += map.len() as u64;
        let map: Box<[(u16, u32)]> = map.into();
        let map_id = match self.maps.get(&map) {
            Some(&id) => id,
            None => {
                // The map is held twice, as a key and in the list.
                self.memory.take(2 * size_of_val(&*map) as u64)?;
                self.memory.room_in(&mut self.maps)?;
                self.memory.room(&mut self.map_list, 1)?;
                let id = id_after(self.map_list.len())?;
                self.map_list.push(map.clone());
                self.maps.insert(map, id);
                id
            }
        };
        self.memory.room(&mut self.captures, 1)?;
        self.captures.push(map_id);
        self.context = self.context_of(self.column, map_id)?;
        Ok(())
    }

    /// Where a `Mark` and a skip came before a column check, go back to the position the `Mark`
    /// kept: the check looks past ignored text, but what follows starts where the skip did.
    fn take_back(&mut self, skip: bool) {
        if skip && let Some(Entry::Mark { pos }) = self.stack.pop() {
            self.pos = pos;
        }
    }

    /// The column of a position: one for each code point from the start of its line, a tab to
    /// the next multiple of the tab width, a form feed back to 0. A carriage return before a
    /// line feed ends its line after the line feed. Finding the start and counting are work of
    /// the bytes passed, as for a literal.
    fn column_at(&mut self, pos: u32) -> u32 {
        let bytes = self.input.as_bytes();
        let pos = (pos as usize).min(bytes.len());
        let mut start = pos;
        while let Some(&before) = start.checked_sub(1).and_then(|at| bytes.get(at)) {
            match before {
                b'\n' => break,
                b'\r' if bytes.get(start) != Some(&b'\n') => break,
                _ => start -= 1,
            }
        }
        let tab = self.program.tab_width;
        let mut column = 0u32;
        for &byte in bytes.get(start..pos).unwrap_or_default() {
            column = match byte {
                b'\t' if tab > 0 => (column / tab + 1).saturating_mul(tab),
                0x0C => 0,
                // A byte that continues a code point adds nothing.
                _ if byte & 0xC0 == 0x80 => column,
                _ => column.saturating_add(1),
            };
        }
        self.steps += (pos - start) as u64 / COLUMN_BYTES_PER_STEP;
        column
    }

    /// Whether a skip at the position surely ends there: the skip expression cannot start with
    /// the next byte, or the input ends.
    fn skips_nothing(&self) -> bool {
        self.program.skip_first.is_some_and(|first| {
            let next = self.input.as_bytes().get(self.pos as usize);
            !next.is_some_and(|&byte| first.contains(byte))
        })
    }

    /// The context the skip cache keys by: the context where the skip expression reads it.
    fn skip_context(&self) -> u32 {
        match self.program.skip_context {
            true => self.context,
            false => 0,
        }
    }

    /// The state of a skip that starts at `start` in the machine as it stands.
    fn skip_state(&self, start: u32) -> SkipState {
        SkipState {
            start,
            flags: self.flags,
            context: self.skip_context(),
            floor: self.floor,
            looking: self.unattempted(),
        }
    }

    /// Where a skip from the position ends, where the last skip started from the same state.
    fn cached_skip_end(&self) -> Option<u32> {
        (self.skip_cache)
            .filter(|last| last.state == self.skip_state(self.pos))
            .map(|last| last.end)
    }

    /// Whether the next byte, at the position or after the skip, may start the code a test
    /// stands before. A skip that the skip cache cannot answer runs in that code, and the
    /// test lets it. A mismatch records the failures of the tokens the code would have tried
    /// there, and pops the entry of the loop the test is in, where the test says so.
    fn next_byte_fits(&mut self, index: u32) -> bool {
        let program = self.program;
        let Some(test) = program.tests.get(index as usize) else {
            return true;
        };
        let at = match (test.after_skip, program.skip_class) {
            (false, _) => self.pos,
            (true, Some(class)) => self.pos + self.run_of(class).0 as u32,
            (true, None) if self.skips_nothing() => self.pos,
            (true, None) => match self.cached_skip_end() {
                Some(end) => end,
                None => return true,
            },
        };
        let next = self.input.as_bytes().get(at as usize);
        let fits = next.is_some_and(|&byte| test.bytes.contains(byte));
        if !fits {
            for &kind in &test.kinds {
                self.record(at, Expected::Kind(kind));
            }
            if test.pop {
                self.stack.pop();
            }
        }
        fits
    }

    /// The length in bytes of the longest run of code points of the class at the position, and
    /// how many it holds. Its bytes count as the bytes of a literal do.
    fn run_of(&mut self, class: u32) -> (usize, u32) {
        let program = self.program;
        let Some(class) = program.classes.get(class as usize) else {
            return (0, 0);
        };
        let rest = self.rest();
        let (mut len, mut count) = (0, 0);
        while let Some(next) = (rest.get(len..)).and_then(|r| class.matches(r, &mut self.steps)) {
            len += next;
            count += 1;
        }
        self.steps += len as u64 / LITERAL_BYTES_PER_STEP;
        (len, count)
    }

    fn literal(&self, index: u32) -> &'p [u8] {
        self.program
            .strings
            .get(index as usize)
            .map_or(&[], |literal| literal)
    }

    /// Push an entry and go on.
    fn push(&mut self, entry: Entry) -> Result<(), Stop> {
        self.entry(entry)?;
        self.pc += 1;
        Ok(())
    }

    /// Push an entry onto the stack.
    fn entry(&mut self, entry: Entry) -> Result<(), Stop> {
        self.memory.room(&mut self.stack, 1)?;
        self.stack.push(entry);
        Ok(())
    }

    /// What an entry pushed now gives back when it catches a failure.
    fn saved(&self) -> Saved {
        Saved {
            pos: self.pos,
            items: self.items.len() as u32,
            nodes: self.nodes.len() as u32,
        }
    }

    /// Keep the items that the newest loop made since it started as its seed, which ends at
    /// the position, in place of the seed before.
    fn keep_seed(&mut self) -> Result<(), Stop> {
        let Some(&Seeded { items, seed }) = self.loops.last() else {
            return Ok(());
        };
        let seed = self.seed_from(items, seed)?;
        if let Some(state) = self.loops.last_mut() {
            state.seed = Some(seed);
        }
        Ok(())
    }

    /// Move the items made above the height `items` into the seeds in place of the seed `old`,
    /// and give the new seed, which ends at the position.
    fn seed_from(&mut self, items: u32, old: Option<Match>) -> Result<Match, Stop> {
        let made = self.items.get(items as usize..).unwrap_or_default();
        self.steps += made.len() as u64;
        let first = old.map_or(self.seeds.len(), |seed| seed.first as usize);
        self.seeds.truncate(first);
        // Indices are 32 bits wide. Every kept item costs a step, so a limit of steps stops
        // first.
        let Ok(first) = u32::try_from(first) else {
            return Err(Stop::Limit(Limit::Steps));
        };
        self.memory.room(&mut self.seeds, made.len())?;
        self.seeds.extend_from_slice(made);
        let seed = Match {
            end: self.pos,
            first,
            count: made.len() as u32,
        };
        self.items.truncate(items as usize);
        Ok(seed)
    }

    /// Take the newest loop off, with its seed.
    fn drop_loop(&mut self) {
        if let Some(seed) = self.loops.pop().and_then(|state| state.seed) {
            self.seeds.truncate(seed.first as usize);
        }
    }

    /// Call a rule that is remembered or on a left cycle: give what the table of left recursion
    /// or the memo holds for it here, or push a frame and run its routine.
    fn call_frame(&mut self, rule: u32, recursive: bool) -> Result<(), Stop> {
        // The verifier holds the index inside the rules.
        let Some(info) = self.program.rules.get(rule as usize) else {
            return Err(Stop::NoMatch);
        };
        let key = MemoKey {
            rule,
            pos: self.pos,
            flags: self.flags,
            context: self.context,
            quiet: self.quiet > 0,
            looking: self.unattempted(),
        };
        let entry = recursive.then(|| self.active(rule)).flatten();
        if let Some(profiler) = self.profiler.as_deref_mut() {
            let seen = (
                rule,
                key.pos,
                key.flags,
                key.context,
                key.quiet,
                key.looking,
            );
            profiler.called(seen, entry.is_some(), &mut self.memory)?;
        }
        if let Some(index) = entry {
            return self.use_entry(index);
        }
        if info.memo
            && let Some(stored) = self.memo.get(&key).copied()
            && self.usable(stored)
        {
            if let Some(profiler) = self.profiler.as_deref_mut() {
                profiler.hit(rule);
            }
            let Some(found) = stored.result else {
                return self.fail();
            };
            // Giving a result again is work of its length, and may put it under two parents.
            self.steps += u64::from(found.count);
            self.shared = true;
            let items = self.memo_items.get(found.range()).unwrap_or_default();
            self.memory.room(&mut self.items, items.len())?;
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
        self.memory.room(&mut self.frames, 1)?;
        self.frames.push(Frame {
            ret: self.pc + 1,
            key,
            items: self.items.len() as u32,
            memo: info.memo,
            recursion,
            outer_dependency: std::mem::replace(&mut self.dependency, NO_FRAME),
        });
        if let Some(profiler) = self.profiler.as_deref_mut() {
            let (stack, nodes) = (self.stack.len(), self.nodes.len());
            profiler.entered(rule, self.pos, self.steps, stack, nodes);
        }
        self.entry(Entry::Frame)?;
        self.pc = info.pc;
        Ok(())
    }

    /// The frame of `rule` that started at this position in this context, if there is one: its
    /// entry of the table of left recursion. The frames that started here are the newest, so
    /// the search ends at the first that started earlier.
    fn active(&mut self, rule: u32) -> Option<u32> {
        let mut index = self.newest_recursion;
        let context = self.context;
        while let Some(frame) = self.frames.get(index as usize) {
            if frame.key.pos != self.pos {
                break;
            }
            if frame.key.rule == rule
                && frame.key.flags == self.flags
                && frame.key.context == context
            {
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
    /// the position is another. An empty seed that holds items can be given at every call in
    /// the pass, and its nodes stand under as many parents, as those of a remembered match do.
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
        self.shared |= seed.end == self.pos && seed.count > 0;
        let items = self.seeds.get(seed.range()).unwrap_or_default();
        self.memory.room(&mut self.items, items.len())?;
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
                    self.back_to_seed(frame, seed, recursion.seed_nodes)?;
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
        // Node indices are 32 bits wide too, and every node costs a step.
        let Ok(seed_nodes) = u32::try_from(self.nodes.len()) else {
            return Err(Stop::Limit(Limit::Steps));
        };
        let seed = self.seed_from(frame.items, recursion.seed)?;
        // The first seed is no growth yet.
        if recursion.seed.is_some()
            && let Some(profiler) = self.profiler.as_deref_mut()
        {
            profiler.grew(frame.key.rule);
        }
        self.pos = frame.key.pos;
        if let Some(newest) = self.frames.last_mut() {
            newest.recursion = Some(Recursion {
                seed: Some(seed),
                seed_nodes,
                used: false,
                ..recursion
            });
        }
        self.entry(Entry::Frame)?;
        self.pc = info.pc;
        Ok(())
    }

    /// Make the seed of a frame the result of its call: the items and the arena as they stood
    /// after the seed, whose nodes lie below everything a later pass made.
    fn back_to_seed(&mut self, frame: Frame, seed: Match, seed_nodes: u32) -> Result<(), Stop> {
        self.restore(Saved {
            pos: seed.end,
            items: frame.items,
            nodes: seed_nodes,
        });
        let items = self.seeds.get(seed.range()).unwrap_or_default();
        self.memory.room(&mut self.items, items.len())?;
        self.items.extend_from_slice(items);
        Ok(())
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
        self.profile_left(!matched);
        if !(frame.memo && independent) {
            return Ok(());
        }
        if let Some(profiler) = self.profiler.as_deref_mut() {
            profiler.stored(frame.key.rule);
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
            self.memory.room(&mut self.memo_items, made.len())?;
            self.memo_items.extend_from_slice(made);
            result = Some(Match {
                end: self.pos,
                first,
                count: made.len() as u32,
            });
        }
        let made = self.serial;
        self.memory.room_in(&mut self.memo)?;
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
        if let Some(profiler) = self.profiler.as_deref_mut() {
            profiler.failed(self.pos);
        }
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
                    self.profile_left(true);
                    None
                }
                Entry::Frame => {
                    let recursion = self.frames.last().and_then(|frame| frame.recursion);
                    if let (Some(&frame), Some(recursion)) = (self.frames.last(), recursion)
                        && let Some(seed) = recursion.seed
                    {
                        // A pass that fails leaves the seed as the result of the call.
                        self.end_failed_token(Some(self.stack.len()));
                        self.back_to_seed(frame, seed, recursion.seed_nodes)?;
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
                Entry::Column { column } => {
                    self.end_column(column)?;
                    None
                }
                Entry::Def { .. } => None,
                Entry::Captures { height, context } => {
                    self.give_back(height, context);
                    None
                }
                Entry::Loop => {
                    self.drop_loop();
                    None
                }
                Entry::Recovering => return self.recovery_failed(),
            };
            if let Some((alt, saved, quiet)) = caught {
                self.end_failed_token(Some(self.stack.len()));
                self.restore(saved);
                if let Some(quiet) = quiet {
                    self.quiet = quiet;
                    self.looking -= 1;
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
                Entry::Call { .. }
                | Entry::Frame
                | Entry::Loop
                | Entry::Predicate { .. }
                | Entry::Recovering => break,
                Entry::Label { .. }
                | Entry::Sub { .. }
                | Entry::Open { .. }
                | Entry::Mark { .. }
                | Entry::Flags { .. }
                | Entry::Column { .. }
                | Entry::Def { .. }
                | Entry::Captures { .. } => passed += 1,
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
        let (position, floor) = (self.pos, self.floor);
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
                    self.looking -= 1;
                    self.pc = alt;
                    return Ok(());
                }
                Entry::Call { .. } => {
                    self.depth -= 1;
                    self.profile_left(false);
                }
                // An error is no result to remember.
                Entry::Frame => {
                    self.drop_frame();
                    self.profile_left(false);
                }
                Entry::Flags { saved } => self.flags = saved,
                Entry::Column { column } => self.end_column(column)?,
                Entry::Captures { height, context } => self.give_back(height, context),
                Entry::Loop => self.drop_loop(),
                Entry::Recovering => {
                    if let Some(frame) = self.recovering_frames.pop() {
                        self.floor = frame.floor;
                        self.context = frame.context;
                    }
                }
                // The skip routine, which a `Skip` before its return called, ends here too.
                Entry::Sub { ret } => {
                    let called = ret
                        .checked_sub(1)
                        .and_then(|at| self.program.code.get(at as usize));
                    if let Some(profiler) = self.profiler.as_deref_mut()
                        && matches!(called, Some(Inst::Skip))
                    {
                        profiler.skip_ended(self.steps);
                    }
                }
                _ => {}
            }
        }
        if self.recovering {
            self.ended = self.recoveries_among()?;
        }
        Err(Stop::Error {
            label,
            position,
            floor,
        })
    }

    /// Recover from the labeled error with this index where the parse recovers: outside a
    /// lookahead and the skip routine, which raise the quiet count, outside a token, and past
    /// the floor, run the label's recovery routine with the position as the floor. Otherwise
    /// stop the parse as `Throw` does. Cold, as every path of recovery, so that the loop of the
    /// machine keeps its shape for a parse in normal mode.
    #[cold]
    fn recover(&mut self, label: u32, field: FieldId) -> Result<(), Stop> {
        let start = self.pos;
        let normal = !self.recovering || self.quiet > 0 || self.token.is_some();
        let past_floor = self.floor.is_none_or(|floor| start > floor);
        let routine = (self.program.recoveries.get(label as usize)).copied();
        let Some(routine) = routine.flatten().filter(|_| !normal && past_floor) else {
            return self.throw(label);
        };
        if let Some(furthest) = &mut self.furthest {
            // What the error says is fixed where the label first fails here, since a later
            // failure there may be a remembered match's, which records nothing again.
            self.memory.room_in(furthest)?;
            self.memory.room_in(&mut self.explanations)?;
            let reached = furthest
                .get(&self.floor)
                .filter(|furthest| furthest.at >= start);
            let explanation = reached.cloned().unwrap_or(Furthest {
                at: start,
                expected: BTreeSet::new(),
            });
            (self.explanations)
                .entry((label, start, self.floor))
                .or_insert(explanation);
        }
        self.memory.room(&mut self.recovering_frames, 1)?;
        self.recovering_frames.push(RecoveryFrame {
            ret: self.pc + 1,
            label,
            field,
            start,
            items: self.items.len() as u32,
            floor: self.floor,
            context: self.context,
        });
        self.entry(Entry::Recovering)?;
        self.floor = Some(start);
        self.context = self.context_of(self.column, self.map())?;
        self.pc = routine;
        Ok(())
    }

    /// End a recovery that matched: what it made becomes the error item of its label, a node
    /// of kind `<error>` from where it started to the position, or of kind `<missing>` where it
    /// matched nothing, which has no children and becomes a token when the tree is laid out.
    #[cold]
    fn recovered(&mut self) -> Result<(), Stop> {
        let Some(frame) = self.recovering_frames.pop() else {
            return Err(Stop::NoMatch);
        };
        self.floor = frame.floor;
        self.context = frame.context;
        let kind = if self.pos == frame.start {
            self.items.truncate(frame.items as usize);
            KindId::MISSING
        } else {
            KindId::ERROR
        };
        self.close_node(kind, frame.start, frame.items as usize)?;
        let node = (self.nodes.len() - 1) as u32;
        if let Some(item) = self.items.last_mut() {
            item.start = frame.start;
            item.end = self.pos;
            item.field = frame.field;
            // It counts for the span of its parent as a token does.
            item.flags = HAS_TOKEN;
        }
        self.memory.room_in(&mut self.labels)?;
        self.labels.insert(node, (frame.label, frame.floor));
        self.pc = frame.ret;
        Ok(())
    }

    /// A recovery that failed leaves the error of its label, which ends the parse.
    #[cold]
    fn recovery_failed(&mut self) -> Result<(), Stop> {
        let Some(frame) = self.recovering_frames.pop() else {
            return Err(Stop::NoMatch);
        };
        self.end_failed_token(Some(self.stack.len()));
        self.floor = frame.floor;
        self.context = frame.context;
        self.items.truncate(frame.items as usize);
        self.pos = frame.start;
        self.throw(frame.label)
    }

    /// The error items among the items of the parse at any depth, in the order of the text, or
    /// a limit where listing those that shared nodes repeat takes more steps than are left. Its
    /// stack holds at most the items the machine holds already.
    #[cold]
    fn recoveries_among(&self) -> Result<Vec<Found>, Stop> {
        let mut found = Vec::new();
        // Only a recovery makes an error item.
        if self.labels.is_empty() {
            return Ok(found);
        }
        let mut memory = self.memory;
        // Which nodes hold an error item at any depth. Children lie before their parents.
        let mut holds: Vec<bool> = Vec::new();
        memory.room(&mut holds, self.nodes.len())?;
        for node in &self.nodes {
            let error = node.kind == KindId::ERROR || node.kind == KindId::MISSING;
            let mut children = (node.first as usize..(node.first + node.count) as usize)
                .filter_map(|index| self.children.get(index));
            let held = error || children.any(|child| holding(&holds, child));
            holds.push(held);
        }
        // A node given to several parents counts under each, as in the tree, so a visit beyond
        // one per node costs what an entry repeating it costs the tree.
        let left = self.limits.max_steps.saturating_sub(self.steps);
        let mut visits = (self.nodes.len() as u64).saturating_add(left / TREE_ENTRY_STEPS);
        let roots = self.items.iter().rev().filter(|item| holding(&holds, item));
        let mut pending: Vec<Item> = roots.copied().collect();
        while let Some(item) = pending.pop() {
            visits = visits.checked_sub(1).ok_or(Stop::Limit(Limit::Steps))?;
            let Some(node) = self.nodes.get(item.value as usize) else {
                continue;
            };
            if node.kind == KindId::ERROR || node.kind == KindId::MISSING {
                memory.room(&mut found, 1)?;
                found.push(self.recovery_of(&item));
            }
            let children = (node.first as usize..(node.first + node.count) as usize)
                .filter_map(|index| self.children.get(index))
                .filter(|child| holding(&holds, child));
            let at = pending.len();
            pending.extend(children.copied());
            pending[at..].reverse();
        }
        Ok(found)
    }

    /// An error item as a recovery, with the name of its label and the floor it failed under.
    fn recovery_of(&self, item: &Item) -> Found {
        let (index, floor) = self.labels.get(&item.value).copied().unzip();
        let label = index.and_then(|label| self.program.labels.get(label as usize));
        Found {
            label: index.unwrap_or_default(),
            recovery: Recovery {
                label: label.map_or_else(String::new, |label| label.text().to_owned()),
                start: item.start as usize,
                end: item.end as usize,
                position: item.start as usize,
                expected: Vec::new(),
            },
            floor: floor.flatten().map(|floor| floor as usize),
        }
    }

    fn record(&mut self, pos: u32, what: Expected) {
        if self.quiet > 0 {
            return;
        }
        if self.floor.is_none() && self.farthest.is_none_or(|farthest| pos > farthest) {
            self.farthest = Some(pos);
        }
        if let Some(expected) = self.diagnose.get_mut(&(pos, self.floor)) {
            expected.insert(what);
        }
        if let Some(furthest) = &mut self.furthest {
            let furthest = furthest.entry(self.floor).or_insert(Furthest {
                at: pos,
                expected: BTreeSet::new(),
            });
            if pos > furthest.at {
                furthest.at = pos;
                furthest.expected.clear();
            }
            if pos == furthest.at {
                furthest.expected.insert(what);
            }
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
        self.memory.room(&mut self.children, collected.len())?;
        self.memory.room(&mut self.nodes, 1)?;
        self.children.extend_from_slice(collected);
        self.nodes.push(Node {
            kind,
            first,
            count: collected.len() as u32,
        });
        self.items.truncate(height);
        self.memory.room(&mut self.items, 1)?;
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

    /// How many entries the arena holds: its nodes, and the tokens among their children and at
    /// the top. A tree that repeats no node has no more.
    fn arena_entries(&self) -> u64 {
        let tokens = |items: &[Item]| items.iter().filter(|item| item.flags & TOKEN != 0).count();
        (self.nodes.len() + tokens(&self.children) + tokens(&self.items)) as u64
    }

    /// Lay out the items of a successful parse as the top level of a tree, in postorder, into
    /// columns sized first. A tree that may repeat shared nodes is counted exactly, and refused
    /// before any of it is built where its repeats would take more steps than the parse has left.
    /// What the arena holds, the steps of the parse made. Any other tree has at most the
    /// arena's entries. The recoveries among the items come beside the tree, which holds none.
    pub fn tree(&self) -> Result<(Tree, Vec<Found>), Limit> {
        // What the layout holds besides the machine's own vectors counts as they do.
        let mut memory = self.memory;
        let entries = if self.shared {
            let sizes = self.nodes.len() * size_of::<u64>();
            memory.take(sizes as u64).map_err(|_| Limit::Memory)?;
            let left = self.limits.max_steps.saturating_sub(self.steps);
            let entries = self.tree_entries();
            let repeats = entries.saturating_sub(self.arena_entries());
            if entries > u64::from(u32::MAX) || repeats.saturating_mul(TREE_ENTRY_STEPS) > left {
                return Err(Limit::Steps);
            }
            entries
        } else {
            self.arena_entries()
        };
        let columns = entries.saturating_mul(crate::tree::ENTRY_BYTES);
        memory.take(columns).map_err(|_| Limit::Memory)?;
        let mut tree = Tree::empty(Arc::clone(&self.program.names));
        tree.reserve(entries as usize);
        let mut recoveries = Vec::new();
        // The nodes being laid out, with the index of the next child of each and the length of
        // the tree when it started. An explicit stack, since trees can be as deep as long.
        let mut open: Vec<(Item, u32, usize)> = Vec::new();
        for &top in &self.items {
            memory.room(&mut open, 1).map_err(|_| Limit::Memory)?;
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
                // Only a parse that recovers makes error items, listed as they are entered.
                if next == 0 && node.kind.0 < KindId::RESERVED {
                    memory.room(&mut recoveries, 1).map_err(|_| Limit::Memory)?;
                    recoveries.push(self.recovery_of(&item));
                    if node.kind == KindId::MISSING {
                        open.pop();
                        tree.push_token(KindId::MISSING, false, item.start, item.end, item.field);
                        continue;
                    }
                }
                if next < node.count {
                    if let Some(last) = open.last_mut() {
                        last.1 += 1;
                    }
                    if let Some(&child) = self.children.get((node.first + next) as usize) {
                        memory.room(&mut open, 1).map_err(|_| Limit::Memory)?;
                        open.push((child, 0, tree.len()));
                    }
                } else {
                    open.pop();
                    let size = (tree.len() - started + 1) as u32;
                    tree.push_node(node.kind, item.start, item.end, size, item.field);
                }
            }
        }
        Ok((tree, recoveries))
    }
}
