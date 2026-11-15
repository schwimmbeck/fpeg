// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The verifier: checks that a program keeps the rules the machine relies on, so that a bug of
//! the compiler shows as a message and not as a wrong parse. The tests run it on every program
//! they compile.
//!
//! Each routine is followed along all its paths, keeping the shape of the stack it built. Every
//! jump stays inside the program and inside its routine, every entry is popped by the
//! instruction made for it, a routine returns with the stack, the token and the quiet count as
//! it found them, every instruction is reached with one shape only, and every index names an
//! entry of its table.

use std::collections::{HashMap, HashSet};

use super::program::{Inst, Program, RuleInfo};
use crate::resolve::MAX_FLAGS;

/// The entries of the machine's stack, as far as the verifier tells them apart. Calls are
/// left out, since every call returns before the next instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Choice,
    Label,
    Predicate,
    Repetition,
    Open,
    Mark,
    Flags,
    Loop,
    Column,
    Def,
    Captures,
}

/// What holds before an instruction: the entries its routine pushed, oldest first, whether a
/// token is open, and how many `QuietOn` are not yet undone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Shape {
    entries: Vec<Entry>,
    token: bool,
    quiet: u32,
}

impl Shape {
    fn with(&self, entry: Entry) -> Shape {
        let mut shape = self.clone();
        shape.entries.push(entry);
        shape
    }
}

/// Check a program, or describe the first problem found.
pub(crate) fn verify(program: &Program) -> Result<(), String> {
    let rules: HashMap<u32, &RuleInfo> = program.rules.iter().map(|r| (r.pc, r)).collect();
    let shared: HashSet<u32> = program.shared.iter().copied().collect();
    let mut roots: Vec<u32> = program.rules.iter().map(|r| r.pc).collect();
    roots.extend(&program.shared);
    roots.extend(program.skip);
    roots.extend(program.recoveries.iter().flatten());
    roots.extend(program.error_tests.iter().flatten());
    let mut entries: Vec<u32> = program.entries.values().copied().collect();
    entries.sort_unstable();
    roots.extend(entries);
    let tables = Tables {
        program,
        rules: &rules,
        shared: &shared,
    };
    let mut seen: Vec<Option<(usize, Shape)>> = vec![None; program.code.len()];
    for (routine, &root) in roots.iter().enumerate() {
        let mut pending = vec![(root, Shape::default())];
        while let Some((pc, shape)) = pending.pop() {
            let Some(&inst) = program.code.get(pc as usize) else {
                return Err(format!("a jump leaves the program at {pc}"));
            };
            match &seen[pc as usize] {
                Some((owner, _)) if *owner != routine => {
                    return Err(format!("{pc} belongs to two routines"));
                }
                Some((_, known)) if *known == shape => continue,
                Some(_) => return Err(format!("{pc} is reached with two shapes of the stack")),
                None => seen[pc as usize] = Some((routine, shape.clone())),
            }
            let next = tables
                .step(pc, inst, shape)
                .map_err(|problem| format!("{pc} {inst:?}: {problem}"))?;
            pending.extend(next);
        }
    }
    Ok(())
}

struct Tables<'p> {
    program: &'p Program,
    /// The rules by the entry of their routine.
    rules: &'p HashMap<u32, &'p RuleInfo>,
    shared: &'p HashSet<u32>,
}

impl Tables<'_> {
    /// The instructions that can follow `inst` with the shape before each. Failures resume at
    /// the target of the entry that catches them, which its push already followed.
    fn step(&self, pc: u32, inst: Inst, mut shape: Shape) -> Result<Vec<(u32, Shape)>, String> {
        let p = self.program;
        let next = pc + 1;
        let index = |index: u32, len: usize, what: &str| match (index as usize) < len {
            true => Ok(()),
            false => Err(format!("no {what} {index}")),
        };
        let top = |shape: &Shape, wanted: &[Entry]| match shape.entries.last() {
            Some(entry) if wanted.contains(entry) => Ok(()),
            other => Err(format!("expects {wanted:?} on top, finds {other:?}")),
        };
        let popped = |shape: &Shape| {
            let mut shape = shape.clone();
            shape.entries.pop();
            shape
        };
        // A column check or a capture after a skip takes back the position of the `Mark` before
        // the skip.
        let taken_back = |shape: Shape, skip: bool| match skip {
            true => top(&shape, &[Entry::Mark]).map(|()| popped(&shape)),
            false => Ok(shape),
        };
        Ok(match inst {
            Inst::Char(_) | Inst::Any | Inst::WordEnd | Inst::Cut => vec![(next, shape)],
            Inst::Str(i) | Inst::StrFold(i) => {
                index(i, p.strings.len(), "literal")?;
                vec![(next, shape)]
            }
            Inst::Class(i) | Inst::Span { class: i, .. } => {
                index(i, p.classes.len(), "class")?;
                vec![(next, shape)]
            }
            Inst::Test { test, .. } | Inst::TestFail(test)
                if p.tests.get(test as usize).is_some_and(|t| t.after_skip)
                    && p.skip.is_none()
                    && p.skip_class.is_none() =>
            {
                return Err("tests after a skip in a grammar that ignores nothing".into());
            }
            Inst::Test { test, target } => {
                index(test, p.tests.len(), "test")?;
                let jumped = match p.tests[test as usize].pop {
                    true => {
                        top(&shape, &[Entry::Choice])?;
                        popped(&shape)
                    }
                    false => shape.clone(),
                };
                vec![(next, shape), (target, jumped)]
            }
            Inst::TestFail(test) => {
                index(test, p.tests.len(), "test")?;
                if p.tests[test as usize].pop {
                    return Err("pops before it fails".into());
                }
                vec![(next, shape)]
            }
            Inst::SeedOpen => vec![(next, shape.with(Entry::Loop))],
            Inst::SeedKeep => {
                top(&shape, &[Entry::Loop])?;
                vec![(next, shape)]
            }
            Inst::SeedUse => match shape.entries.contains(&Entry::Loop) {
                true => vec![(next, shape)],
                false => return Err("uses a seed outside a loop".into()),
            },
            Inst::SeedGrow(target) => {
                top(&shape, &[Entry::Choice])?;
                vec![(target, popped(&shape))]
            }
            Inst::SeedClose => {
                top(&shape, &[Entry::Loop])?;
                vec![(next, popped(&shape))]
            }
            Inst::Choice(target) => vec![(next, shape.with(Entry::Choice)), (target, shape)],
            Inst::LabelChoice(target) => vec![(next, shape.with(Entry::Label)), (target, shape)],
            Inst::PredChoice(target) => {
                vec![(next, shape.with(Entry::Predicate)), (target, shape)]
            }
            Inst::RepOpen { end, bounds } => {
                index(bounds, p.bounds.len(), "bounds")?;
                vec![(next, shape.with(Entry::Repetition)), (end, shape)]
            }
            Inst::Commit(target) => {
                top(&shape, &[Entry::Choice, Entry::Label])?;
                vec![(target, popped(&shape))]
            }
            Inst::LoopCommit(target) => {
                top(&shape, &[Entry::Choice])?;
                vec![(next, popped(&shape)), (target, shape)]
            }
            Inst::RepNext(target) => {
                top(&shape, &[Entry::Repetition])?;
                vec![(next, popped(&shape)), (target, shape)]
            }
            Inst::BackCommit(target) => {
                top(&shape, &[Entry::Predicate])?;
                vec![(target, popped(&shape))]
            }
            Inst::FailTwice => {
                top(&shape, &[Entry::Predicate])?;
                Vec::new()
            }
            Inst::ErrorAttempt(alternative) => {
                index(alternative, p.error_messages.len(), "error alternative")?;
                Vec::new()
            }
            Inst::Throw(label) => {
                index(label, p.labels.len(), "label")?;
                Vec::new()
            }
            // A recovery returns before the next instruction, as a call does.
            Inst::Recover { label, field } => {
                index(label, p.labels.len(), "label")?;
                index(u32::from(field.0), p.names.fields.len(), "field")?;
                match p.recoveries.get(label as usize) {
                    Some(Some(_)) => vec![(next, shape)],
                    _ => return Err("recovers a label that has no recovery".into()),
                }
            }
            Inst::Fail => Vec::new(),
            // A rule on a left cycle needs the frame of `LrCall`, a remembered one that of
            // `MemoCall`.
            Inst::Call(target) => match self.rules.get(&target) {
                Some(info) if info.recursive || info.memo => {
                    return Err("calls a rule that needs a frame".into());
                }
                Some(_) => vec![(next, shape)],
                None => return Err("calls no rule".into()),
            },
            Inst::MemoCall(rule) => match p.rules.get(rule as usize) {
                Some(info) if info.recursive => {
                    return Err("remembers a rule that is left recursive".into());
                }
                Some(info) if info.memo => vec![(next, shape)],
                Some(_) => return Err("remembers a rule that is not remembered".into()),
                None => return Err(format!("no rule {rule}")),
            },
            Inst::LrCall(rule) => match p.rules.get(rule as usize) {
                Some(info) if info.recursive => vec![(next, shape)],
                Some(_) => return Err("grows a rule that is not left recursive".into()),
                None => return Err(format!("no rule {rule}")),
            },
            Inst::Sub(target) => match self.shared.contains(&target) {
                true => vec![(next, shape)],
                false => return Err("calls no shared part".into()),
            },
            Inst::Skip => match (p.skip, p.skip_class) {
                (None, None) => return Err("skips in a grammar that ignores nothing".into()),
                (_, Some(class)) => {
                    index(class, p.classes.len(), "class")?;
                    vec![(next, shape)]
                }
                (Some(_), None) => vec![(next, shape)],
            },
            Inst::Ret | Inst::End | Inst::TestEnd => match shape == Shape::default() {
                true => Vec::new(),
                false => return Err(format!("leaves {shape:?}")),
            },
            Inst::QuietOn => {
                shape.quiet += 1;
                vec![(next, shape)]
            }
            Inst::QuietOff => match shape.quiet.checked_sub(1) {
                Some(quiet) => {
                    shape.quiet = quiet;
                    vec![(next, shape)]
                }
                None => return Err("undoes no QuietOn".into()),
            },
            Inst::TokenOpen(kind) => {
                index(kind.0, p.names.kinds.len(), "kind")?;
                if shape.token {
                    return Err("opens a token inside a token".into());
                }
                shape.token = true;
                vec![(next, shape)]
            }
            Inst::TokenClose { .. } => {
                if !shape.token {
                    return Err("closes no token".into());
                }
                shape.token = false;
                vec![(next, shape)]
            }
            Inst::Open => vec![(next, shape.with(Entry::Open))],
            Inst::CloseNode(kind) | Inst::CloseTransparent(kind) => {
                index(kind.0, p.names.kinds.len(), "kind")?;
                top(&shape, &[Entry::Open])?;
                vec![(next, popped(&shape))]
            }
            Inst::CloseField(field) => {
                index(u32::from(field.0), p.names.fields.len(), "field")?;
                top(&shape, &[Entry::Open])?;
                vec![(next, popped(&shape))]
            }
            Inst::Mark => vec![(next, shape.with(Entry::Mark))],
            Inst::Unreserved => {
                top(&shape, &[Entry::Mark])?;
                vec![(next, popped(&shape))]
            }
            Inst::FlagOn(flag) | Inst::FlagOff(flag) => {
                index(u32::from(flag), MAX_FLAGS, "flag")?;
                vec![(next, shape.with(Entry::Flags))]
            }
            Inst::FlagsPop => {
                top(&shape, &[Entry::Flags])?;
                vec![(next, popped(&shape))]
            }
            Inst::FlagTest { flag, .. } => {
                index(u32::from(flag), MAX_FLAGS, "flag")?;
                vec![(next, shape)]
            }
            Inst::Indent { skip } => vec![(next, taken_back(shape, skip)?.with(Entry::Column))],
            Inst::IndentEnd => {
                top(&shape, &[Entry::Column])?;
                vec![(next, popped(&shape))]
            }
            Inst::Same { skip } => vec![(next, taken_back(shape, skip)?)],
            Inst::DefOpen { skip } => vec![(next, taken_back(shape, skip)?.with(Entry::Def))],
            Inst::DefClose(_) => {
                top(&shape, &[Entry::Def])?;
                vec![(next, popped(&shape))]
            }
            Inst::Ref(_) => vec![(next, shape)],
            Inst::CaptureMark => vec![(next, shape.with(Entry::Captures))],
            Inst::CaptureKeep | Inst::CaptureDrop => {
                top(&shape, &[Entry::Captures])?;
                vec![(next, popped(&shape))]
            }
        })
    }
}
