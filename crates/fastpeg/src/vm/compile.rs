// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The compiler: from the core language of a resolved grammar to a program.
//!
//! Each core form becomes a fixed piece of code, close to LPeg's. A shared part becomes a
//! routine of its own that every use calls, and every repetition compiles its expression once,
//! so the program grows linearly with the grammar.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::program::{Class, Inst, Program, RuleInfo};
use crate::analysis;
use crate::grammar::{GrammarError, Span};
use crate::ir::{self, RuleId};
use crate::resolve::{MAX_ERRORS, MAX_FLAGS};

/// Compile a resolved grammar, or report what the machine cannot run yet.
pub(crate) fn compile(grammar: &ir::Grammar) -> Result<Program, Vec<GrammarError>> {
    let errors = unsupported(grammar);
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut c = Compiler {
        has_skip: grammar.skip.is_some(),
        code: Vec::new(),
        strings: Vec::new(),
        string_ids: HashMap::new(),
        classes: Vec::new(),
        bounds: Vec::new(),
        calls: Vec::new(),
        shared: HashMap::new(),
        queue: Vec::new(),
        shared_pcs: Vec::new(),
        subs: Vec::new(),
    };
    let mut rules = Vec::new();
    for rule in &grammar.rules {
        rules.push(RuleInfo {
            name: rule.name.clone(),
            pc: c.here(),
        });
        c.expr(&rule.body);
        c.emit(Inst::Ret);
    }
    let skip = grammar.skip.as_ref().map(|skip| {
        // The skip expression in plain mode, as often as it consumes input, recording nothing.
        let pc = c.here();
        c.emit(Inst::QuietOn);
        let choice = c.emit(Inst::Choice(0));
        let step = c.here();
        c.expr(skip);
        c.emit(Inst::LoopCommit(step));
        c.patch(choice);
        c.emit(Inst::QuietOff);
        c.emit(Inst::Ret);
        pc
    });
    let mut names: Vec<&String> = grammar.entry.keys().collect();
    names.sort();
    let mut entries = HashMap::new();
    for name in names {
        entries.insert(name.clone(), c.here());
        c.expr(&grammar.entry[name]);
        if c.has_skip {
            c.emit(Inst::Skip);
        }
        c.emit(Inst::End);
    }
    while let Some((index, expr)) = c.queue.pop() {
        c.shared_pcs[index] = c.here();
        c.expr(&expr);
        c.emit(Inst::Ret);
    }
    for (at, rule) in std::mem::take(&mut c.calls) {
        c.code[at] = match rules.get(rule.0 as usize) {
            Some(info) => Inst::Call(info.pc),
            // A call to a rule that does not exist fails, as the reference interpreter has it.
            None => Inst::Fail,
        };
    }
    for (at, index) in std::mem::take(&mut c.subs) {
        c.code[at] = Inst::Sub(c.shared_pcs[index]);
    }
    if u32::try_from(c.code.len()).is_err() {
        let message = "the grammar is too large to compile".to_owned();
        return Err(vec![GrammarError {
            message,
            span: Span::default(),
        }]);
    }
    let labels = (grammar.labels.iter())
        .map(|label| match label {
            ir::Label::Message(text) | ir::Label::Named(text) => text.clone(),
        })
        .collect();
    Ok(Program {
        code: c.code,
        strings: c.strings,
        classes: c.classes,
        bounds: c.bounds,
        rules,
        shared: c.shared_pcs,
        skip,
        entries,
        word: Class::new(&grammar.word),
        reserved: ir::Reserved {
            exact: grammar.reserved.exact.clone(),
            folded: grammar.reserved.folded.clone(),
        },
        names: Arc::clone(&grammar.names),
        kind_display: grammar.kind_display.clone(),
        labels,
    })
}

/// The bit of a flag in the machine's flag register. A resolved grammar has at most 64 flags,
/// so every flag has one.
fn flag_bit(flag: ir::FlagId) -> Option<u8> {
    u8::try_from(flag.0)
        .ok()
        .filter(|&bit| usize::from(bit) < MAX_FLAGS)
}

/// The core forms of a rule body that the machine does not run yet.
#[derive(Default)]
struct Uses {
    cut: bool,
    columns: bool,
    captures: bool,
}

impl Uses {
    /// Note the forms in `expr`, visiting each shared part once in the whole grammar.
    fn scan(&mut self, expr: &ir::Expr, seen: &mut HashSet<*const ir::Expr>) {
        use ir::Expr as E;
        match expr {
            E::Cut => self.cut = true,
            E::Same { .. } => self.columns = true,
            E::Ref { .. } => self.captures = true,
            E::Indent { expr, .. } => {
                self.columns = true;
                self.scan(expr, seen);
            }
            E::Def { expr, .. } => {
                self.captures = true;
                self.scan(expr, seen);
            }
            E::Seq(items) | E::Choice(items) => {
                for item in items {
                    self.scan(item, seen);
                }
            }
            E::Repeat { expr, .. }
            | E::And(expr)
            | E::Not(expr)
            | E::Token { expr, .. }
            | E::Node { expr, .. }
            | E::Transparent { expr, .. }
            | E::Field { expr, .. }
            | E::Unreserved(expr)
            | E::FlagOn { expr, .. }
            | E::FlagOff { expr, .. }
            | E::Label { expr, .. } => self.scan(expr, seen),
            E::Shared(expr) => {
                if seen.insert(Arc::as_ptr(expr)) {
                    self.scan(expr, seen);
                }
            }
            E::Empty
            | E::Literal { .. }
            | E::Class(_)
            | E::Any
            | E::Call(_)
            | E::Skip
            | E::WordEnd
            | E::FlagTest { .. }
            | E::ErrorAlternative => {}
        }
    }

    /// The messages for what `who` uses, with the milestone that brings it.
    fn messages(&self, who: &str) -> Vec<String> {
        let mut messages = Vec::new();
        for (used, what, milestone) in [
            (self.cut, "a cut", "M4"),
            (self.columns, "%indent or %same", "M6"),
            (self.captures, "%def or %ref", "M6"),
        ] {
            if used {
                messages.push(format!(
                    "{who} uses {what}, which the parser does not run yet, see the roadmap \
                     ({milestone})"
                ));
            }
        }
        messages
    }
}

/// Report what the machine does not run yet: left recursion, cuts, columns and captures, the
/// first hundred at most.
fn unsupported(grammar: &ir::Grammar) -> Vec<GrammarError> {
    let facts = analysis::analyze(grammar);
    let mut errors: Vec<GrammarError> = Vec::new();
    let mut add = |message: String, span: Span| {
        // The instances of a template repeat the messages of their template.
        let error = GrammarError { message, span };
        if errors.len() < MAX_ERRORS && !errors.contains(&error) {
            errors.push(error);
        }
    };
    let mut seen = HashSet::new();
    for (rule, left_recursive) in grammar.rules.iter().zip(facts.left_recursive) {
        let who = format!("the rule {}", rule.name);
        if left_recursive {
            let message = format!(
                "{who} is left recursive, which the parser does not run yet, see the roadmap (M4)"
            );
            add(message, rule.span);
        }
        let mut uses = Uses::default();
        uses.scan(&rule.body, &mut seen);
        for message in uses.messages(&who) {
            add(message, rule.span);
        }
    }
    if let Some(skip) = &grammar.skip {
        let mut uses = Uses::default();
        uses.scan(skip, &mut seen);
        for message in uses.messages("the %ignore expression") {
            add(message, Span::default());
        }
    }
    errors
}

struct Compiler {
    has_skip: bool,
    code: Vec<Inst>,
    strings: Vec<Box<[u8]>>,
    string_ids: HashMap<Box<[u8]>, u32>,
    classes: Vec<Class>,
    bounds: Vec<(u32, u32)>,
    /// Calls to point at the routine of the rule they call, once every rule has one.
    calls: Vec<(usize, RuleId)>,
    /// The index of each shared part met so far, and the parts whose routines are still to
    /// be compiled.
    shared: HashMap<*const ir::Expr, usize>,
    queue: Vec<(usize, Arc<ir::Expr>)>,
    shared_pcs: Vec<u32>,
    /// Calls to point at the routine of the shared part they call, once every part has one.
    subs: Vec<(usize, usize)>,
}

impl Compiler {
    fn here(&self) -> u32 {
        // The length is checked against u32 once the program is complete.
        self.code.len() as u32
    }

    /// Append an instruction and return where it stands.
    fn emit(&mut self, inst: Inst) -> usize {
        self.code.push(inst);
        self.code.len() - 1
    }

    /// Point the jump at `at` to the current end of the code.
    fn patch(&mut self, at: usize) {
        let target = self.here();
        match &mut self.code[at] {
            Inst::Choice(t)
            | Inst::Commit(t)
            | Inst::PredChoice(t)
            | Inst::BackCommit(t)
            | Inst::LabelChoice(t)
            | Inst::RepOpen { end: t, .. } => *t = target,
            _ => {}
        }
    }

    /// Compile an expression. It recurses as deeply as the expression nests, which the
    /// resolver bounds.
    fn expr(&mut self, expr: &ir::Expr) {
        use ir::Expr as E;
        match expr {
            E::Empty => {}
            E::Literal {
                text,
                case_insensitive,
            } => self.literal(text.as_bytes(), *case_insensitive),
            E::Class(set) => {
                self.classes.push(Class::new(set));
                self.emit(Inst::Class((self.classes.len() - 1) as u32));
            }
            E::Any => {
                self.emit(Inst::Any);
            }
            E::Call(rule) => {
                let at = self.emit(Inst::Call(0));
                self.calls.push((at, *rule));
            }
            E::Seq(parts) => {
                for part in parts {
                    self.expr(part);
                }
            }
            E::Choice(alternatives) => self.choice(alternatives),
            E::Repeat { expr, min, max } => self.repeat(expr, *min, *max),
            E::And(inner) => {
                let choice = self.emit(Inst::PredChoice(0));
                self.expr(inner);
                let back = self.emit(Inst::BackCommit(0));
                self.patch(choice);
                self.emit(Inst::Fail);
                self.patch(back);
            }
            E::Not(inner) => {
                let choice = self.emit(Inst::PredChoice(0));
                self.expr(inner);
                self.emit(Inst::FailTwice);
                self.patch(choice);
            }
            E::Skip => {
                if self.has_skip {
                    self.emit(Inst::Skip);
                }
            }
            E::Token { kind, hidden, expr } => {
                self.emit(Inst::TokenOpen(*kind));
                self.expr(expr);
                self.emit(Inst::TokenClose { hidden: *hidden });
            }
            E::Node { kind, expr } => self.collect(expr, Inst::CloseNode(*kind)),
            E::Transparent { kind, expr } => self.collect(expr, Inst::CloseTransparent(*kind)),
            E::Field { field, expr } => self.collect(expr, Inst::CloseField(*field)),
            E::WordEnd => {
                self.emit(Inst::WordEnd);
            }
            E::Unreserved(inner) => {
                self.emit(Inst::Mark);
                self.expr(inner);
                self.emit(Inst::Unreserved);
            }
            E::FlagOn { flag, expr } => match flag_bit(*flag) {
                Some(bit) => {
                    self.emit(Inst::FlagOn(bit));
                    self.expr(expr);
                    self.emit(Inst::FlagsPop);
                }
                None => {
                    self.emit(Inst::Fail);
                }
            },
            E::FlagOff { flag, expr } => match flag_bit(*flag) {
                Some(bit) => {
                    self.emit(Inst::FlagOff(bit));
                    self.expr(expr);
                    self.emit(Inst::FlagsPop);
                }
                None => {
                    self.emit(Inst::Fail);
                }
            },
            E::FlagTest { flag, negated } => match flag_bit(*flag) {
                Some(flag) => {
                    self.emit(Inst::FlagTest {
                        flag,
                        negated: *negated,
                    });
                }
                None => {
                    self.emit(Inst::Fail);
                }
            },
            E::Label { label, expr, skip } => {
                let choice = self.emit(Inst::LabelChoice(0));
                self.expr(expr);
                let commit = self.emit(Inst::Commit(0));
                self.patch(choice);
                if *skip && self.has_skip {
                    self.emit(Inst::Skip);
                }
                self.emit(Inst::Throw(label.0));
                self.patch(commit);
            }
            E::ErrorAlternative => {
                self.emit(Inst::Fail);
            }
            E::Shared(expr) => {
                let key = Arc::as_ptr(expr);
                let index = match self.shared.get(&key) {
                    Some(&index) => index,
                    None => {
                        let index = self.shared_pcs.len();
                        self.shared_pcs.push(0);
                        self.shared.insert(key, index);
                        self.queue.push((index, Arc::clone(expr)));
                        index
                    }
                };
                let at = self.emit(Inst::Sub(0));
                self.subs.push((at, index));
            }
            // Rejected before compiling, see `unsupported`.
            E::Cut | E::Indent { .. } | E::Same { .. } | E::Def { .. } | E::Ref { .. } => {
                self.emit(Inst::Fail);
            }
        }
    }

    fn literal(&mut self, bytes: &[u8], case_insensitive: bool) {
        let folds = case_insensitive && bytes.iter().any(u8::is_ascii_alphabetic);
        match bytes {
            [] => {}
            [byte] if byte.is_ascii() && !folds => {
                self.emit(Inst::Char(*byte));
            }
            _ => {
                let next = self.strings.len() as u32;
                let id = *self.string_ids.entry(bytes.into()).or_insert(next);
                if id == next {
                    self.strings.push(bytes.into());
                }
                self.emit(if folds {
                    Inst::StrFold(id)
                } else {
                    Inst::Str(id)
                });
            }
        }
    }

    fn choice(&mut self, alternatives: &[ir::Expr]) {
        let Some((last, rest)) = alternatives.split_last() else {
            self.emit(Inst::Fail);
            return;
        };
        let mut commits = Vec::new();
        for alternative in rest {
            let choice = self.emit(Inst::Choice(0));
            self.expr(alternative);
            commits.push(self.emit(Inst::Commit(0)));
            self.patch(choice);
        }
        self.expr(last);
        for commit in commits {
            self.patch(commit);
        }
    }

    fn repeat(&mut self, expr: &ir::Expr, min: u32, max: Option<u32>) {
        match (min, max) {
            (0, Some(0)) => {}
            // The resolver rejects a maximum below the minimum.
            (_, Some(max)) if max < min => {
                self.emit(Inst::Fail);
            }
            (1, Some(1)) => self.expr(expr),
            (0, Some(1)) => {
                let choice = self.emit(Inst::Choice(0));
                self.expr(expr);
                let commit = self.emit(Inst::Commit(0));
                self.patch(choice);
                self.patch(commit);
            }
            (0, None) => {
                let choice = self.emit(Inst::Choice(0));
                let step = self.here();
                self.expr(expr);
                self.emit(Inst::LoopCommit(step));
                self.patch(choice);
            }
            (min, max) => {
                self.bounds.push((min, max.unwrap_or(u32::MAX)));
                let bounds = (self.bounds.len() - 1) as u32;
                let open = self.emit(Inst::RepOpen { end: 0, bounds });
                let step = self.here();
                self.expr(expr);
                self.emit(Inst::RepNext(step));
                self.patch(open);
            }
        }
    }

    /// `Open`, the expression, and the instruction that closes what it collected.
    fn collect(&mut self, expr: &ir::Expr, close: Inst) {
        self.emit(Inst::Open);
        self.expr(expr);
        self.emit(close);
    }
}
