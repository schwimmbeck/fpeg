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

use super::program::Program;

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
    /// The parses that ran out of their budget of steps and started again with every rule
    /// remembered. Their counts hold every run.
    pub restarts: u64,
}

impl Profile {
    /// Add the counts of another parse with the same parser, whose rows are the same.
    pub fn add(&mut self, other: Profile) {
        self.steps += other.steps;
        self.peak_stack = self.peak_stack.max(other.peak_stack);
        self.peak_nodes = self.peak_nodes.max(other.peak_nodes);
        self.memo_entries = self.memo_entries.max(other.memo_entries);
        self.restarts += other.restarts;
        if self.rows.is_empty() {
            self.rows = other.rows;
            return;
        }
        for (sum, row) in self.rows.iter_mut().zip(other.rows) {
            sum.calls += row.calls;
            sum.failures += row.failures;
            sum.steps += row.steps;
            sum.steps_with_callees += row.steps_with_callees;
            sum.backtracked_bytes += row.backtracked_bytes;
            sum.repeats += row.repeats;
            sum.memo_hits += row.memo_hits;
            sum.memo_stores += row.memo_stores;
            sum.growths += row.growths;
        }
    }
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
    /// Where a rule calls itself, or a template another instance of itself, the outermost call
    /// counts.
    pub steps_with_callees: u64,
    /// Bytes that failed calls had got through before they failed: from where each started
    /// to the farthest point one of its tries failed at.
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

/// A call whose routine is running.
struct Active {
    rule: u32,
    /// Where the call started, and the steps of the parse then.
    start: u32,
    entered: u64,
    /// The row that the steps went to before the call, and go to again after it: the rule
    /// that called, or the skip routine, which calls terminals.
    caller: u32,
    /// The farthest position that a try in the call failed at.
    reached: u32,
}

/// The counting state of a profiling parse.
pub(super) struct Profiler {
    /// A row per rule of the program, then the skip routine's, then the parse's own.
    rows: Vec<ProfileRow>,
    /// The rules being matched, innermost last.
    active: Vec<Active>,
    /// For each row, the first row of its name, which the instances of a template share.
    name_of: Vec<u32>,
    /// How many calls of the rules of each name are in progress, by the first row of the name.
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
        let skip_ret = program.skip_return().unwrap_or(u32::MAX);
        let parse = (rows.len() - 1) as u32;
        let mut first: HashMap<&str, u32> = HashMap::new();
        let name_of = (rows.iter().zip(0..))
            .map(|(row, index)| *first.entry(row.name.as_str()).or_insert(index))
            .collect();
        Profiler {
            nesting: vec![0; rows.len()],
            name_of,
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

    /// How many calls of rules with the name of `rule` are in progress.
    fn nesting(&mut self, rule: u32) -> Option<&mut u32> {
        let name = *self.name_of.get(rule as usize)?;
        self.nesting.get_mut(name as usize)
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
        let caller = self.current;
        self.switch(steps, rule);
        self.active.push(Active {
            rule,
            start: pos,
            entered: steps,
            caller,
            reached: pos,
        });
        if let Some(nesting) = self.nesting(rule) {
            *nesting += 1;
        }
        self.peak_stack = self.peak_stack.max(stack as u64);
        self.peak_nodes = self.peak_nodes.max(nodes as u64);
    }

    /// The routine of the newest call is over: it returned, or with `failed_at` it failed
    /// there, or an error passed it.
    pub fn left(&mut self, steps: u64, failed_at: Option<u32>) {
        let Some(call) = self.active.pop() else {
            return;
        };
        let outermost = match self.nesting(call.rule) {
            Some(nesting) => {
                *nesting = nesting.saturating_sub(1);
                *nesting == 0
            }
            None => false,
        };
        if let Some(row) = self.row(call.rule) {
            row.steps_with_callees += if outermost { steps - call.entered } else { 0 };
            if let Some(pos) = failed_at {
                row.failures += 1;
                let reached = call.reached.max(pos);
                row.backtracked_bytes += u64::from(reached.saturating_sub(call.start));
            }
        }
        // The tries of a call are tries of its caller too.
        if let Some(outer) = self.active.last_mut() {
            outer.reached = outer.reached.max(call.reached);
        }
        self.switch(steps, call.caller);
    }

    /// A try failed at `pos`, which the call in progress got as far as.
    pub fn failed(&mut self, pos: u32) {
        if let Some(call) = self.active.last_mut() {
            call.reached = call.reached.max(pos);
        }
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
            self.skip_ended(steps);
        }
    }

    /// The skip routine is over, by its return or by an error that a lookahead caught.
    pub fn skip_ended(&mut self, steps: u64) {
        self.switch(steps, self.skipping_for);
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

    /// A pass of the loop of left recursion that the newest call runs made its match longer.
    pub fn loop_grew(&mut self) {
        if let Some(rule) = self.active.last().map(|call| call.rule) {
            self.grew(rule);
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
            restarts: 0,
        }
    }
}
