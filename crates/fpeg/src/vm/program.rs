// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The program format: what the compiler writes and the parsing machine runs.
//!
//! A program is one flat array of instructions with side tables for literals, character
//! classes, repetition counts and the names of the tree. Every rule, every shared part of a
//! rule and the skip expression is a routine that ends in `Ret`, and every rule that can start
//! a parse has an entry routine that ends in `End`. Jump targets are positions in the array.

use std::collections::HashMap;
use std::sync::Arc;

use crate::ir::{CharSet, Reserved};
use crate::tree::{FieldId, Kind, KindId, Names};

/// One instruction of the parsing machine.
///
/// The machine keeps one stack of entries. An expression that succeeds leaves the stack as it
/// found it. One that fails unwinds the stack to the newest entry that catches the failure,
/// which restores the position and the items made since it was pushed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Inst {
    /// Match one byte, an ASCII character.
    Char(u8),
    /// Match the literal with this index.
    Str(u32),
    /// Match the literal with this index, ASCII letters in either case.
    StrFold(u32),
    /// Match one code point of the class with this index.
    Class(u32),
    /// Match any one code point.
    Any,
    /// Push a choice entry that catches a failure and resumes at the target.
    Choice(u32),
    /// Pop the choice or label entry on top and jump.
    Commit(u32),
    /// End a step of `e*`. If the step consumed input, move the choice entry on top to the
    /// position and items reached and jump back to the step. Otherwise pop it, since a step
    /// that consumes nothing is the last.
    LoopCommit(u32),
    /// Push a repetition entry for the counts with index `bounds`. When it catches a failure
    /// after enough steps, the repetition succeeds at `end`.
    RepOpen {
        /// Where the repetition ends.
        end: u32,
        /// The index of its minimum and maximum count.
        bounds: u32,
    },
    /// End a step of a counted repetition: count it, and jump back to the step unless the
    /// maximum is reached or the step consumed nothing.
    RepNext(u32),
    /// Push a predicate entry: a lookahead starts, and no failure inside it is recorded.
    PredChoice(u32),
    /// The tested expression of `&e` matched: pop the predicate entry, return to its position
    /// and items, and jump.
    BackCommit(u32),
    /// The tested expression of `!e` matched: pop the predicate entry and fail.
    FailTwice,
    /// Push a label entry, which catches every failure and resumes at the target to throw.
    LabelChoice(u32),
    /// Stop the parse with the label with this index at the current position. Inside a
    /// lookahead the error is a failure of the lookahead.
    Throw(u32),
    /// Fail.
    Fail,
    /// Mark the innermost choice or repetition entry of the rule activation, so that a failure
    /// drops it and unwinds on. Calls and lookahead end the search for it.
    Cut,
    /// Call the rule whose routine starts at the target.
    Call(u32),
    /// Call the shared part whose routine starts at the target.
    Sub(u32),
    /// Return from the routine.
    Ret,
    /// Skip ignored text by calling the skip routine.
    Skip,
    /// End the parse: success at the end of the input, otherwise a failure there that
    /// expects the end.
    End,
    /// Stop recording failures, for the skip routine.
    QuietOn,
    /// Record failures again.
    QuietOff,
    /// Open a token of the kind at the current position.
    TokenOpen(KindId),
    /// Close the open token, which is hidden or not.
    TokenClose {
        /// Whether views skip the token.
        hidden: bool,
    },
    /// Start collecting the items of a node or a field.
    Open,
    /// Make the items since the matching `Open` a node of the kind.
    CloseNode(KindId),
    /// The same, unless exactly one of the items is visible, which then stay as they are.
    CloseTransparent(KindId),
    /// Give the field to each item since the matching `Open` that is visible and has none.
    CloseField(FieldId),
    /// Fail when a word character follows.
    WordEnd,
    /// Remember the position for the matching `Unreserved`.
    Mark,
    /// Fail when the text since the matching `Mark` is a reserved word.
    Unreserved,
    /// Turn the flag on until the matching `FlagsPop`.
    FlagOn(u8),
    /// Turn the flag off until the matching `FlagsPop`.
    FlagOff(u8),
    /// Restore the flags as they were before the matching `FlagOn` or `FlagOff`.
    FlagsPop,
    /// Fail unless the flag is on, or off when `negated`.
    FlagTest {
        /// The flag.
        flag: u8,
        /// Whether the flag must be off.
        negated: bool,
    },
}

/// A character class as the machine tests it: one bit for each ASCII character, and what
/// holds beyond ASCII.
#[derive(Clone, Debug)]
pub(crate) struct Class {
    ascii: [u64; 2],
    beyond: Beyond,
    /// The class as the grammar defines it.
    pub set: CharSet,
}

/// Which code points beyond ASCII a class contains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Beyond {
    None,
    All,
    /// Each is tested against the set.
    Some,
}

impl Class {
    pub fn new(set: &CharSet) -> Class {
        let mut ascii = [0u64; 2];
        for byte in 0..0x80u8 {
            if set.contains(char::from(byte)) {
                ascii[usize::from(byte >> 6)] |= 1 << (byte & 63);
            }
        }
        // Ranges within ASCII alone leave every code point beyond it on one side.
        let only_ascii = set.items.iter().all(|item| match *item {
            crate::grammar::ClassItem::Range(_, last) => last.is_ascii(),
            crate::grammar::ClassItem::Property { .. } => false,
        });
        let beyond = match (only_ascii, set.negated) {
            (true, false) => Beyond::None,
            (true, true) => Beyond::All,
            (false, _) => Beyond::Some,
        };
        Class {
            ascii,
            beyond,
            set: set.clone(),
        }
    }

    /// The length in bytes of the code point that starts `rest`, if the class contains it. A
    /// test beyond ASCII that looks through the class's items adds their number to `steps`.
    pub fn matches(&self, rest: &str, steps: &mut u64) -> Option<usize> {
        let &first = rest.as_bytes().first()?;
        if first.is_ascii() {
            let bit = self.ascii[usize::from(first >> 6)] >> (first & 63) & 1;
            return (bit == 1).then_some(1);
        }
        let c = rest.chars().next()?;
        let contained = match self.beyond {
            Beyond::None => false,
            Beyond::All => true,
            Beyond::Some => {
                *steps += self.set.items.len() as u64;
                self.set.contains(c)
            }
        };
        contained.then_some(c.len_utf8())
    }
}

/// A rule's name and where its routine starts.
#[derive(Clone, Debug)]
pub(crate) struct RuleInfo {
    pub name: String,
    pub pc: u32,
}

/// A compiled grammar. See the module documentation.
#[derive(Debug)]
pub(crate) struct Program {
    pub code: Vec<Inst>,
    /// The literals of `Str` and `StrFold`.
    pub strings: Vec<Box<[u8]>>,
    /// The classes of `Class`.
    pub classes: Vec<Class>,
    /// The minimum and maximum counts of `RepOpen`. `u32::MAX` stands for no maximum, which no
    /// repetition can reach, since every counted step consumes input.
    pub bounds: Vec<(u32, u32)>,
    pub rules: Vec<RuleInfo>,
    /// Where the routine of each shared part starts.
    pub shared: Vec<u32>,
    /// Where the skip routine starts, if the grammar ignores text.
    pub skip: Option<u32>,
    /// Where the entry routine of each rule that can start a parse starts.
    pub entries: HashMap<String, u32>,
    /// The word characters of `WordEnd`.
    pub word: Class,
    pub reserved: Reserved,
    pub names: Arc<Names>,
    /// What an expected set shows for each kind.
    pub kind_display: Vec<String>,
    /// The texts of the error labels.
    pub labels: Vec<String>,
    /// The kind table shown to callers.
    pub kinds: Vec<Kind>,
    /// For each entry, whether a parse from it always leaves one visible item.
    pub single_item: HashMap<String, bool>,
}
