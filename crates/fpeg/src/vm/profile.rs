// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a parse did, rule by rule: the counts behind `fpeg profile`.
//!
//! A machine that profiles keeps a [`Profiler`] and tells it of every call, return and failed
//! call. Steps are not counted one by one: whenever the rule in progress changes, the steps
//! since the last change go to the row of the rule they were spent in. The skip routine and
//! the code that starts and ends a parse have rows of their own, so the rows sum to the steps
//! of the parse.

use std::collections::{HashMap, HashSet};

use super::program::{Inst, Program};

/// The name of the row for the skip routine.
const IGNORE_ROW: &str = "%ignore";

/// The name of the row for the code that starts and ends a parse.
const PARSE_ROW: &str = "(parse)";

/// What a parse did. See [`crate::Parser::profile`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    /// A row for each name that rules have, in the order of the grammar, then one for the
    /// ignored text, named `%ignore`, and one for the code that starts and ends the parse,
    /// named `(parse)`. The uses of a rule with parameters share the row of their name.
    pub rows: Vec<ProfileRow>,
    /// The steps of the parse, which are the sum of the steps of the rows.
    pub steps: u64,
    /// The most entries the machine's stack held when a rule was called.
    pub peak_stack: u64,
    /// The most nodes the machine held when a rule was called or the parse ended.
    pub peak_nodes: u64,
    /// The results the parse remembered.
    pub memo_entries: u64,
}

/// What a parse did in the rules of one name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileRow {
    /// The name of the rules.
    pub name: String,
    /// Whether their results are remembered.
    pub remembered: bool,
    /// Calls, also those that a remembered result or a left recursion answered.
    pub calls: u64,
    /// Calls that ran the rule and failed.
    pub failures: u64,
    /// Steps spent in the rule's own code, not in the rules it called.
    pub steps: u64,
    /// Steps from the calls that ran the rule to their ends, the rules it called included.
    /// Where a rule calls itself, the outermost call counts.
    pub steps_with_callees: u64,
    /// Bytes that failed calls had matched and gave back.
    pub backtracked_bytes: u64,
    /// Calls at a position where the rule had been called before, with the same flags and the
    /// same recording of failures: what remembering the rule would answer.
    pub repeats: u64,
    /// Calls that a remembered result answered, a remembered failure included.
    pub memo_hits: u64,
    /// Results that were remembered.
    pub memo_stores: u64,
    /// Passes of a left recursion that made the match longer.
    pub growths: u64,
}

/// A call as the profiler tells calls apart for [`ProfileRow::repeats`].
type Seen = (u32, u32, u64, bool);

/// The counting state of a profiling parse.
pub(super) struct Profiler {
    /// A row per rule of the program, then the skip routine's, then the parse's own.
    rows: Vec<ProfileRow>,
    /// The rules being matched, innermost last, each with where its call started and the
    /// steps of the parse then.
    active: Vec<(u32, u32, u64)>,
    /// How many calls of each rule are in progress.
    nesting: Vec<u32>,
    /// The row that the steps since `mark` belong to, and the steps of the parse at the last
    /// change of that row.
    current: u32,
    mark: u64,
    /// The row to go back to when the skip routine returns.
    skipping_for: u32,
    /// Where the skip routine returns.
    skip_ret: u32,
    seen: HashSet<Seen>,
    peak_stack: u64,
    peak_nodes: u64,
}

impl Profiler {
    pub fn new(program: &Program) -> Profiler {
        let named = |name: &str, remembered| ProfileRow {
            name: name.to_owned(),
            remembered,
            ..ProfileRow::default()
        };
        let mut rows: Vec<ProfileRow> = (program.rules.iter())
            .map(|rule| named(&rule.name, rule.memo))
            .collect();
        rows.extend([named(IGNORE_ROW, false), named(PARSE_ROW, false)]);
        // A routine ends at its first return.
        let skip_ret = program.skip.map_or(u32::MAX, |skip| {
            let rest = program.code.get(skip as usize..).unwrap_or_default();
            let ret = rest.iter().position(|inst| matches!(inst, Inst::Ret));
            skip + ret.unwrap_or(0) as u32
        });
        let parse = (rows.len() - 1) as u32;
        Profiler {
            nesting: vec![0; rows.len()],
            rows,
            active: Vec::new(),
            current: parse,
            mark: 0,
            skipping_for: parse,
            skip_ret,
            seen: HashSet::new(),
            peak_stack: 0,
            peak_nodes: 0,
        }
    }

    fn row(&mut self, rule: u32) -> Option<&mut ProfileRow> {
        self.rows.get_mut(rule as usize)
    }

    /// Give the steps since the last change to the row in progress, and go on with `row`.
    fn switch(&mut self, steps: u64, row: u32) {
        let spent = steps - self.mark;
        if let Some(current) = self.row(self.current) {
            current.steps += spent;
        }
        self.mark = steps;
        self.current = row;
    }

    /// The row of the rule around the newest call, or of the parse itself.
    fn outer(&self) -> u32 {
        let parse = (self.rows.len() - 1) as u32;
        self.active.last().map_or(parse, |&(rule, ..)| rule)
    }

    /// A rule is called: count the call, and whether it repeats one. The left-recursive use of
    /// the entry of a call in progress repeats nothing that remembering the rule would answer.
    pub fn called(&mut self, key: Seen, left_recursive: bool) {
        let repeated = !left_recursive && !self.seen.insert(key);
        if let Some(row) = self.row(key.0) {
            row.calls += 1;
            row.repeats += u64::from(repeated);
        }
    }

    /// The call just counted runs the rule's routine.
    pub fn entered(&mut self, rule: u32, pos: u32, steps: u64, stack: usize, nodes: usize) {
        self.switch(steps, rule);
        self.active.push((rule, pos, steps));
        if let Some(nesting) = self.nesting.get_mut(rule as usize) {
            *nesting += 1;
        }
        self.peak_stack = self.peak_stack.max(stack as u64);
        self.peak_nodes = self.peak_nodes.max(nodes as u64);
    }

    /// The routine of the newest call is over: it returned, or with `failed_at` it failed
    /// there, or an error passed it.
    pub fn left(&mut self, steps: u64, failed_at: Option<u32>) {
        let Some((rule, start, entered)) = self.active.pop() else {
            return;
        };
        let outermost = match self.nesting.get_mut(rule as usize) {
            Some(nesting) => {
                *nesting = nesting.saturating_sub(1);
                *nesting == 0
            }
            None => false,
        };
        if let Some(row) = self.row(rule) {
            row.steps_with_callees += if outermost { steps - entered } else { 0 };
            if let Some(pos) = failed_at {
                row.failures += 1;
                row.backtracked_bytes += u64::from(pos.saturating_sub(start));
            }
        }
        let outer = self.outer();
        self.switch(steps, outer);
    }

    /// The skip routine starts.
    pub fn skip(&mut self, steps: u64) {
        self.skipping_for = self.current;
        let ignore = (self.rows.len() - 2) as u32;
        self.switch(steps, ignore);
    }

    /// A shared part or the skip routine returns from `pc`.
    pub fn returned_from(&mut self, pc: u32, steps: u64) {
        if pc == self.skip_ret {
            self.switch(steps, self.skipping_for);
        }
    }

    /// A remembered result answered a call of `rule`.
    pub fn hit(&mut self, rule: u32) {
        if let Some(row) = self.row(rule) {
            row.memo_hits += 1;
        }
    }

    /// A result of `rule` was remembered.
    pub fn stored(&mut self, rule: u32) {
        if let Some(row) = self.row(rule) {
            row.memo_stores += 1;
        }
    }

    /// A pass of the left recursion of `rule` made its match longer.
    pub fn grew(&mut self, rule: u32) {
        if let Some(row) = self.row(rule) {
            row.growths += 1;
        }
    }

    /// The profile of the parse, which took `steps` steps and ended with `nodes` nodes and
    /// `memo_entries` remembered results.
    pub fn finish(mut self, steps: u64, nodes: usize, memo_entries: usize) -> Profile {
        let current = self.current;
        self.switch(steps, current);
        let mut rows: Vec<ProfileRow> = Vec::new();
        let mut by_name: HashMap<String, usize> = HashMap::new();
        for row in self.rows {
            // The instances of a template share a name and a row.
            let index = *by_name.entry(row.name.clone()).or_insert(rows.len());
            match rows.get_mut(index) {
                Some(known) => {
                    known.remembered |= row.remembered;
                    known.calls += row.calls;
                    known.failures += row.failures;
                    known.steps += row.steps;
                    known.steps_with_callees += row.steps_with_callees;
                    known.backtracked_bytes += row.backtracked_bytes;
                    known.repeats += row.repeats;
                    known.memo_hits += row.memo_hits;
                    known.memo_stores += row.memo_stores;
                    known.growths += row.growths;
                }
                None => rows.push(row),
            }
        }
        Profile {
            rows,
            steps,
            peak_stack: self.peak_stack,
            peak_nodes: self.peak_nodes.max(nodes as u64),
            memo_entries: memo_entries as u64,
        }
    }
}
