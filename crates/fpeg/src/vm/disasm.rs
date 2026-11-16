// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The disassembler: a program as text, one instruction per line under the name of the
//! routine it belongs to, for tests and for the messages of failed differential tests.

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

use super::program::{Inst, Program};
use crate::grammar::print::{expr_text, quoted};
use crate::grammar::{Class, Expr, ExprKind};

impl Display for Program {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut headings: HashMap<u32, String> = HashMap::new();
        for rule in &self.rules {
            headings.insert(rule.pc, format!("rule {}:", rule.name));
        }
        for (index, &pc) in self.shared.iter().enumerate() {
            headings.insert(pc, format!("shared {index}:"));
        }
        if let Some(pc) = self.skip {
            headings.insert(pc, "skip:".to_owned());
        }
        if let Some(pc) = self.skip_test {
            headings.insert(pc, "skip test:".to_owned());
        }
        for (label, pc) in self.recoveries.iter().enumerate() {
            if let (Some(pc), Some(label)) = (pc, self.labels.get(label)) {
                headings.insert(*pc, format!("recover {}:", label.text()));
            }
        }
        for (index, pc) in self.error_tests.iter().enumerate() {
            if let (Some(pc), Some(message)) = (pc, self.error_messages.get(index)) {
                headings.insert(*pc, format!("test {}:", quoted(message)));
            }
        }
        for (name, &pc) in &self.entries {
            headings.insert(pc, format!("entry {name}:"));
        }
        let routines = Routines {
            rules: self.rules.iter().map(|r| (r.pc, r.name.as_str())).collect(),
            shared: (self.shared.iter().enumerate())
                .map(|(index, &pc)| (pc, index))
                .collect(),
        };
        for (pc, inst) in self.code.iter().enumerate() {
            if let Some(heading) = headings.get(&(pc as u32)) {
                writeln!(f, "{heading}")?;
            }
            writeln!(f, "{pc:>6}  {}", self.inst_text(inst, &routines))?;
        }
        if let Some(class) = self.skip_class {
            let run = self.inst_text(&Inst::Span { class, min: 0 }, &routines);
            writeln!(f, "skip:\n{:>6}  {run}", "")?;
        }
        Ok(())
    }
}

/// The names of routines by where they start, for the targets of calls.
struct Routines<'p> {
    rules: HashMap<u32, &'p str>,
    shared: HashMap<u32, usize>,
}

impl Program {
    /// A test as its bytes in ranges, as a class shows them, and whether it pops.
    fn test_text(&self, index: u32) -> String {
        let Some(test) = self.tests.get(index as usize) else {
            return format!("?{index}");
        };
        let shown = |byte: u8| match byte {
            b' '..=b'~' if !b"[]\\-^".contains(&byte) => char::from(byte).to_string(),
            b'\t' => "\\t".to_owned(),
            b'\n' => "\\n".to_owned(),
            b'\r' => "\\r".to_owned(),
            _ if byte.is_ascii_graphic() => format!("\\{}", char::from(byte)),
            _ => format!("\\x{byte:02x}"),
        };
        let mut ranges = String::new();
        let mut byte = 0u16;
        while byte < 256 {
            if !test.bytes.contains(byte as u8) {
                byte += 1;
                continue;
            }
            let first = byte as u8;
            while byte < 256 && test.bytes.contains(byte as u8) {
                byte += 1;
            }
            let last = (byte - 1) as u8;
            ranges += &shown(first);
            if last > first {
                ranges += &format!("-{}", shown(last));
            }
        }
        let pop = if test.pop { "pop " } else { "" };
        let skip = if test.after_skip { "skip " } else { "" };
        let mut text = format!("{pop}{skip}[{ranges}]");
        if !test.kinds.is_empty() {
            text += " records";
            for kind in &test.kinds {
                text += &format!(" {}", self.kind_name(kind.0));
            }
        }
        text
    }

    fn kind_name(&self, kind: u32) -> &str {
        self.names
            .kinds
            .get(kind as usize)
            .map_or("?", String::as_str)
    }

    fn inst_text(&self, inst: &Inst, routines: &Routines<'_>) -> String {
        let string = |index: u32| {
            let bytes = self.strings.get(index as usize).map_or(&[][..], |s| s);
            quoted(&String::from_utf8_lossy(bytes))
        };
        let class = |index: u32| match self.classes.get(index as usize) {
            Some(class) => expr_text(&Expr::new(ExprKind::Class(Class {
                negated: class.set.negated,
                items: class.set.items.clone(),
            }))),
            None => format!("?{index}"),
        };
        match *inst {
            Inst::Char(byte) => format!("Char {:?}", char::from(byte)),
            Inst::Str(index) => format!("Str {}", string(index)),
            Inst::StrFold(index) => format!("StrFold {}", string(index)),
            Inst::Class(index) => format!("Class {}", class(index)),
            Inst::Span { class: index, min } => format!("Span {} {{{min},}}", class(index)),
            Inst::Any => "Any".to_owned(),
            Inst::Test { test, target } => format!("Test {} {target}", self.test_text(test)),
            Inst::TestFail(test) => format!("TestFail {}", self.test_text(test)),
            Inst::SeedOpen => "SeedOpen".to_owned(),
            Inst::SeedKeep => "SeedKeep".to_owned(),
            Inst::SeedUse => "SeedUse".to_owned(),
            Inst::SeedGrow(target) => format!("SeedGrow {target}"),
            Inst::SeedClose => "SeedClose".to_owned(),
            Inst::Choice(target) => format!("Choice {target}"),
            Inst::Commit(target) => format!("Commit {target}"),
            Inst::LoopCommit(target) => format!("LoopCommit {target}"),
            Inst::RepOpen { end, bounds } => {
                let counts = match self.bounds.get(bounds as usize) {
                    Some(&(min, u32::MAX)) => format!("{{{min},}}"),
                    Some(&(min, max)) if min == max => format!("{{{min}}}"),
                    Some(&(min, max)) => format!("{{{min},{max}}}"),
                    None => format!("?{bounds}"),
                };
                format!("RepOpen {counts} {end}")
            }
            Inst::RepNext(target) => format!("RepNext {target}"),
            Inst::PredChoice(target) => format!("PredChoice {target}"),
            Inst::BackCommit(target) => format!("BackCommit {target}"),
            Inst::FailTwice => "FailTwice".to_owned(),
            Inst::ErrorAttempt(index) => match self.error_messages.get(index as usize) {
                Some(text) => format!("ErrorAttempt {}", quoted(text)),
                None => format!("ErrorAttempt ?{index}"),
            },
            Inst::TestEnd => "TestEnd".to_owned(),
            Inst::LabelChoice(target) => format!("LabelChoice {target}"),
            Inst::Throw(label) => match self.labels.get(label as usize) {
                Some(label) => format!("Throw {}", quoted(label.text())),
                None => format!("Throw ?{label}"),
            },
            Inst::Recover { label, field } => {
                let label = match self.labels.get(label as usize) {
                    Some(label) => quoted(label.text()),
                    None => format!("?{label}"),
                };
                match self.names.fields.get(usize::from(field.0)) {
                    Some(name) if field.0 != 0 => format!("Recover {label} {name}"),
                    _ => format!("Recover {label}"),
                }
            }
            Inst::Fail => "Fail".to_owned(),
            Inst::Cut => "Cut".to_owned(),
            Inst::Call(target) => match routines.rules.get(&target) {
                Some(name) => format!("Call {name}"),
                None => format!("Call ?{target}"),
            },
            Inst::MemoCall(rule) => match self.rules.get(rule as usize) {
                Some(info) => format!("MemoCall {}", info.name),
                None => format!("MemoCall ?{rule}"),
            },
            Inst::LrCall(rule) => match self.rules.get(rule as usize) {
                Some(info) => format!("LrCall {}", info.name),
                None => format!("LrCall ?{rule}"),
            },
            Inst::Sub(target) => match routines.shared.get(&target) {
                Some(index) => format!("Sub shared {index}"),
                None => format!("Sub ?{target}"),
            },
            Inst::Ret => "Ret".to_owned(),
            Inst::Skip => "Skip".to_owned(),
            Inst::End => "End".to_owned(),
            Inst::QuietOn => "QuietOn".to_owned(),
            Inst::QuietOff => "QuietOff".to_owned(),
            Inst::TokenOpen(kind) => format!("TokenOpen {}", self.kind_name(kind.0)),
            Inst::TokenClose { hidden: true } => "TokenClose hidden".to_owned(),
            Inst::TokenClose { hidden: false } => "TokenClose".to_owned(),
            Inst::Open => "Open".to_owned(),
            Inst::CloseNode(kind) => format!("CloseNode {}", self.kind_name(kind.0)),
            Inst::CloseTransparent(kind) => {
                format!("CloseTransparent {}", self.kind_name(kind.0))
            }
            Inst::CloseField(field) => {
                let name = self.names.fields.get(usize::from(field.0));
                format!("CloseField {}", name.map_or("?", String::as_str))
            }
            Inst::WordEnd => "WordEnd".to_owned(),
            Inst::Mark => "Mark".to_owned(),
            Inst::Unreserved => "Unreserved".to_owned(),
            Inst::FlagOn(flag) => format!("FlagOn {flag}"),
            Inst::FlagOff(flag) => format!("FlagOff {flag}"),
            Inst::FlagsPop => "FlagsPop".to_owned(),
            Inst::FlagTest {
                flag,
                negated: false,
            } => format!("FlagTest {flag}"),
            Inst::FlagTest {
                flag,
                negated: true,
            } => format!("FlagTest !{flag}"),
            Inst::Indent { skip } => format!("Indent{}", if skip { " skip" } else { "" }),
            Inst::IndentEnd => "IndentEnd".to_owned(),
            Inst::Same { skip } => format!("Same{}", if skip { " skip" } else { "" }),
            Inst::AtEnd { skip } => format!("AtEnd{}", if skip { " skip" } else { "" }),
            Inst::DefOpen { skip } => format!("DefOpen{}", if skip { " skip" } else { "" }),
            Inst::DefClose(capture) => format!("DefClose {capture}"),
            Inst::Ref(capture) => format!("Ref {capture}"),
            Inst::CaptureMark => "CaptureMark".to_owned(),
            Inst::CaptureKeep => "CaptureKeep".to_owned(),
            Inst::CaptureDrop => "CaptureDrop".to_owned(),
        }
    }
}
