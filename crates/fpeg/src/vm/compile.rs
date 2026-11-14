// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The compiler: from the core language of a resolved grammar to a program.
//!
//! Each core form becomes a fixed piece of code, close to LPeg's. A shared part becomes a
//! routine of its own that every use calls, and every repetition compiles its expression once,
//! so the program grows linearly with the grammar.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use super::program::{ByteTest, Class, Inst, Program, RuleInfo};
use super::{Memo, Options, Passes};
use crate::analysis::{self, Bytes, Start, Starts};
use crate::diag::{Diagnostic, DiagnosticCode};
use crate::grammar::{ClassItem, Span};
use crate::ir::{self, RuleId};
use crate::resolve::MAX_FLAGS;
use crate::tree::KindId;

/// The most expressions a rule may have, counting those of the rules it inlines in turn,
/// for its calls to compile as its body.
const MAX_INLINE_EXPRESSIONS: usize = 24;

/// Compile a resolved grammar, or report that it is too large to compile.
pub(crate) fn compile(
    grammar: &ir::Grammar,
    options: &Options,
) -> Result<Program, Vec<Diagnostic>> {
    let facts = analysis::analyze(grammar);
    let mut c = Compiler {
        has_skip: grammar.skip.is_some(),
        code: Vec::new(),
        strings: Vec::new(),
        string_ids: HashMap::new(),
        strings_of: HashMap::new(),
        classes: Vec::new(),
        classes_of: HashMap::new(),
        bounds: Vec::new(),
        calls: Vec::new(),
        shared: HashMap::new(),
        queue: Vec::new(),
        shared_pcs: Vec::new(),
        subs: Vec::new(),
        cuttable: false,
        free_cuts: HashMap::new(),
        passes: options.passes,
        starts: None,
        tests: Vec::new(),
        test_ids: HashMap::new(),
        rules: &grammar.rules,
        inline: Vec::new(),
        seed_rule: None,
        defines_shared: HashMap::new(),
        defining: Vec::new(),
    };
    c.defining = (grammar.rules.iter())
        .map(|rule| c.defines(&rule.body))
        .collect();
    let loops: Vec<Option<Loop<'_>>> = (0..grammar.rules.len())
        .map(|i| {
            // Each pass of the semantics starts with the captures of the call, which the passes
            // of a loop would have to give back.
            let candidate = options.passes.left_loop && facts.left_recursive[i] && !c.defining[i];
            candidate
                .then(|| c.loop_of(RuleId(i as u32), &facts))
                .flatten()
        })
        .collect();
    // A rule that runs as a loop has no frame of left recursion.
    let growing: Vec<bool> = (facts.left_recursive.iter().zip(&loops))
        .map(|(&recursive, looped)| recursive && looped.is_none())
        .collect();
    // What expressions start with serves dispatch, and the skip, which ends at once at a byte its
    // expression cannot start with.
    let starts =
        (options.passes.dispatch || options.passes.skip).then(|| facts.starts(grammar, &growing));
    let skip_first = skip_first(grammar, options, starts.as_ref());
    if options.passes.dispatch {
        c.starts = starts;
    }
    let twice = facts.tried_twice(grammar);
    let memo: Vec<bool> = (0..grammar.rules.len())
        .map(|i| remembered(options.memo, &grammar.rules[i], growing[i] || twice[i]))
        .collect();
    if options.passes.inline {
        c.inline = c.inlinable(&memo, &facts.recursive);
    }
    let mut rules = Vec::new();
    for (i, rule) in grammar.rules.iter().enumerate() {
        rules.push(RuleInfo {
            name: rule.name.clone(),
            pc: c.here(),
            memo: memo[i],
            recursive: growing[i],
        });
        // A cut ends at the call of its rule, and so do its captures.
        c.cuttable = false;
        if c.defining[i] {
            c.emit(Inst::CaptureMark);
        }
        match &loops[i] {
            Some(looped) => c.left_loop(RuleId(i as u32), looped),
            None => c.expr(&rule.body),
        }
        if c.defining[i] {
            c.emit(Inst::CaptureDrop);
        }
        c.emit(Inst::Ret);
    }
    // Skipping is `e*` of the skip expression `e`, a span where `e` repeats a class.
    let skip_class = match &grammar.skip {
        Some(skip) if options.passes.skip => {
            span(skip, 0).map(|(points, set, _)| c.class(points, &set))
        }
        _ => None,
    };
    let skip = grammar
        .skip
        .as_ref()
        .filter(|_| skip_class.is_none())
        .map(|skip| {
            // The skip expression in plain mode, as often as it consumes input, recording nothing.
            let pc = c.here();
            c.emit(Inst::QuietOn);
            let choice = c.emit(Inst::Choice(0));
            let step = c.here();
            c.cuttable = true;
            // Where the next byte cannot start the skip expression, the skipping ends.
            let test = (c.test_for(skip, true)).map(|test| c.emit(Inst::Test { test, target: 0 }));
            // What a skip captures holds for that skip alone.
            let defines = c.defines(skip);
            if defines {
                c.emit(Inst::CaptureMark);
            }
            // A failure after a cut ends the skipping as any other does, so it must not pass the
            // entry of the loop.
            c.cut_scope(skip);
            if defines {
                c.emit(Inst::CaptureDrop);
            }
            c.emit(Inst::LoopCommit(step));
            c.patch(choice);
            if let Some(test) = test {
                c.patch(test);
            }
            c.emit(Inst::QuietOff);
            c.emit(Inst::Ret);
            pc
        });
    let mut names: Vec<&String> = grammar.entry.keys().collect();
    names.sort();
    let mut entries = HashMap::new();
    // An entry runs once a parse, so it calls its rule rather than holding a copy of it.
    let inline = std::mem::take(&mut c.inline);
    for name in names {
        entries.insert(name.clone(), c.here());
        c.cuttable = false;
        c.expr(&grammar.entry[name]);
        if c.has_skip {
            c.emit(Inst::Skip);
        }
        c.emit(Inst::End);
    }
    c.inline = inline;
    while let Some((index, expr)) = c.queue.pop() {
        c.shared_pcs[index] = c.here();
        // A cut passes the call of a shared part, and what stands around its uses is not
        // known here.
        c.cuttable = true;
        c.expr(expr);
        c.emit(Inst::Ret);
    }
    for (at, rule) in std::mem::take(&mut c.calls) {
        c.code[at] = match rules.get(rule.0 as usize) {
            Some(info) if info.recursive => Inst::LrCall(rule.0),
            Some(info) if info.memo => Inst::MemoCall(rule.0),
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
        return Err(vec![Diagnostic {
            code: DiagnosticCode::TooLarge,
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
        tests: c.tests,
        bounds: c.bounds,
        rules,
        shared: c.shared_pcs,
        skip,
        skip_class,
        entries,
        word: Class::new(&grammar.word),
        reserved: ir::Reserved {
            exact: grammar.reserved.exact.clone(),
            folded: grammar.reserved.folded.clone(),
        },
        names: Arc::clone(&grammar.names),
        kind_display: grammar.kind_display.clone(),
        labels,
        kinds: grammar.kinds(),
        single_item: grammar.single_item.clone(),
        passes: options.passes,
        tab_width: grammar.tab_width,
        skip_first,
        // A skip that checks a column or matches a capture may depend on the context. Where the
        // captures stay inside the terminals it calls, the context at a skip is 0 all the same.
        skip_context: (grammar.skip.as_ref()).is_some_and(|skip| {
            analysis::reaches(grammar, skip, |expr| {
                matches!(
                    expr,
                    ir::Expr::Indent { .. } | ir::Expr::Same { .. } | ir::Expr::Ref { .. }
                )
            })
        }),
    })
}

/// Whether the results of a rule are remembered, `repeated` where by default it is: where a
/// choice point can call it twice at one position, or it keeps the growing loop of left
/// recursion, whose pass that finds the match grown no further runs its body again.
fn remembered(memo: Memo, rule: &ir::Rule, repeated: bool) -> bool {
    match (memo, rule.memo) {
        (Memo::Off, _) | (Memo::Grammar, ir::MemoMark::NoMemo) => false,
        (Memo::All, _) | (Memo::Grammar, ir::MemoMark::Memo) => true,
        (Memo::Grammar, ir::MemoMark::None) => repeated,
    }
}

/// The first bytes of the skip expression, where the skip pass runs its routine and the
/// expression cannot match nothing and fails plainly at its first byte: a skip at any other byte
/// ends where it starts, whatever the flags and the context, since the bytes hold for them all.
fn skip_first(
    grammar: &ir::Grammar,
    options: &Options,
    starts: Option<&Starts<'_>>,
) -> Option<Bytes> {
    let skip = grammar.skip.as_ref()?;
    if !options.passes.skip || span(skip, 0).is_some() {
        return None;
    }
    let start = starts?.of(skip);
    (!start.nullable && start.plain && start.bytes != Bytes::ALL).then_some(start.bytes)
}

/// The bit of a flag in the machine's flag register. A resolved grammar has at most 64 flags,
/// so every flag has one.
fn flag_bit(flag: ir::FlagId) -> Option<u8> {
    u8::try_from(flag.0)
        .ok()
        .filter(|&bit| usize::from(bit) < MAX_FLAGS)
}

struct Compiler<'f> {
    has_skip: bool,
    code: Vec<Inst>,
    strings: Vec<Box<[u8]>>,
    string_ids: HashMap<Box<[u8]>, u32>,
    /// The string and the class of each literal and class compiled so far, by its address,
    /// since a body compiled into its callers compiles again at each call.
    strings_of: HashMap<*const ir::Expr, u32>,
    classes: Vec<Class>,
    classes_of: HashMap<*const ir::Expr, u32>,
    bounds: Vec<(u32, u32)>,
    /// Calls to point at the routine of the rule they call, once every rule has one.
    calls: Vec<(usize, RuleId)>,
    /// The index of each shared part met so far, and the parts whose routines are still to
    /// be compiled.
    shared: HashMap<*const ir::Expr, usize>,
    queue: Vec<(usize, &'f ir::Expr)>,
    shared_pcs: Vec<u32>,
    /// Calls to point at the routine of the shared part they call, once every part has one.
    subs: Vec<(usize, usize)>,
    /// Whether a cut in the code being compiled could mark an entry of the routine: one of a
    /// choice or repetition around it, with no lookahead between.
    cuttable: bool,
    /// Whether each shared part met so far holds a free cut.
    free_cuts: HashMap<*const ir::Expr, bool>,
    passes: Passes,
    /// What each expression can start with, where dispatch is on.
    starts: Option<Starts<'f>>,
    tests: Vec<ByteTest>,
    test_ids: HashMap<ByteTest, u32>,
    rules: &'f [ir::Rule],
    /// Whether the calls of each rule compile as its body.
    inline: Vec<bool>,
    /// The rule whose next call compiles as `SeedUse`: the leading call of a recursive
    /// alternative of a loop.
    seed_rule: Option<RuleId>,
    /// Whether each shared part met so far can make a capture.
    defines_shared: HashMap<*const ir::Expr, bool>,
    /// Whether each rule can make a capture in its own activation.
    defining: Vec<bool>,
}

/// A rule that runs as a loop: its recursive alternatives, each with the parts that follow
/// its leading call, and its base alternatives.
struct Loop<'f> {
    recursive: &'f [ir::Expr],
    rests: Vec<&'f [ir::Expr]>,
    base: &'f [ir::Expr],
}

/// The parts that follow the leading call of `rule` in `expr`, if `expr` starts with one:
/// through nodes and fields, and as the first part of a sequence.
fn after_call(expr: &ir::Expr, rule: RuleId) -> Option<&[ir::Expr]> {
    match expr {
        ir::Expr::Call(called) if *called == rule => Some(&[]),
        ir::Expr::Node { expr, .. }
        | ir::Expr::Transparent { expr, .. }
        | ir::Expr::Field { expr, .. } => after_call(expr, rule),
        ir::Expr::Seq(parts) => {
            let (first, rest) = parts.split_first()?;
            match after_call(first, rule)? {
                [] => Some(rest),
                // A call nested deeper than the first part of this sequence has parts of its
                // own after it, which this form does not take apart.
                _ => None,
            }
        }
        _ => None,
    }
}

/// The code points of a class, or of a literal of one code point that ignores no case.
fn span_set(expr: &ir::Expr) -> Option<Cow<'_, ir::CharSet>> {
    match expr {
        ir::Expr::Class(set) => Some(Cow::Borrowed(set)),
        ir::Expr::Literal {
            text,
            case_insensitive: false,
        } => {
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(Cow::Owned(ir::CharSet {
                    negated: false,
                    items: vec![ClassItem::Range(c, c)],
                })),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The expression of the code points, the code points and the fewest of them for a span that
/// matches `e{min,}` exactly: `c{min,}` of a class or one code point `c`, `(c*){min,}`, `(c+)*`
/// and `(c+)+`. A step that matches nothing is the last and satisfies the minimum, so
/// `(c*){min,}` takes the run whatever `min` says, while `(c+){2,}` never matches, since its
/// first step takes the whole run.
fn span(expr: &ir::Expr, min: u32) -> Option<(&ir::Expr, Cow<'_, ir::CharSet>, u32)> {
    let (points, min) = match expr {
        ir::Expr::Repeat {
            expr: inner,
            min: 0,
            max: None,
        } => (&**inner, 0),
        ir::Expr::Repeat {
            expr: inner,
            min: 1,
            max: None,
        } if min <= 1 => (&**inner, min),
        _ => (expr, min),
    };
    span_set(points).map(|set| (points, set, min))
}

impl<'f> Compiler<'f> {
    /// The rules whose calls compile as their bodies: rules that are not remembered, call
    /// themselves nowhere and hold no free cut, which would reach past the call, with at
    /// most [`MAX_INLINE_EXPRESSIONS`] expressions, a call they inline counted with the body
    /// it compiles as. So a call never compiles to more, and a chain of rules that call each
    /// other breaks into calls every so many links. Each candidate is sized once those it
    /// calls are, on an explicit stack, since chains of rules can be as long as the grammar.
    fn inlinable(&mut self, memo: &[bool], recursive: &[bool]) -> Vec<bool> {
        // The captures a rule makes end with its activation, which an inlined body has not.
        let candidate: Vec<bool> = (0..self.rules.len())
            .map(|i| {
                !memo[i]
                    && !recursive[i]
                    && !self.defining[i]
                    && !self.free_cut(&self.rules[i].body)
            })
            .collect();
        let mut sizes: Vec<Option<usize>> = vec![None; self.rules.len()];
        for root in 0..self.rules.len() {
            let mut stack = vec![root];
            while let Some(&rule) = stack.last() {
                if !candidate[rule] || sizes[rule].is_some() {
                    stack.pop();
                    continue;
                }
                match self.inlined_size(rule, &candidate, &sizes) {
                    Ok(size) => sizes[rule] = Some(size),
                    Err(callee) => stack.push(callee),
                }
            }
        }
        let fits = |size: Option<usize>| size.is_some_and(|size| size <= MAX_INLINE_EXPRESSIONS);
        (0..self.rules.len()).map(|i| fits(sizes[i])).collect()
    }

    /// How many expressions the body of a candidate compiles to, at most one past the most
    /// that fit, or a candidate it calls that is not sized yet. A shared part counts once, as
    /// its call. A repetition of no steps compiles to nothing, and the analysis sees no call in
    /// it, so a candidate may call itself there.
    fn inlined_size(
        &self,
        rule: usize,
        candidate: &[bool],
        sizes: &[Option<usize>],
    ) -> Result<usize, usize> {
        let mut size = 0;
        let mut todo = vec![&self.rules[rule].body];
        while let Some(expr) = todo.pop() {
            if size > MAX_INLINE_EXPRESSIONS {
                break;
            }
            size += 1;
            match expr {
                ir::Expr::Call(callee) if candidate.get(callee.0 as usize) == Some(&true) => {
                    match sizes[callee.0 as usize] {
                        Some(inner) if inner <= MAX_INLINE_EXPRESSIONS => size += inner,
                        Some(_) => {}
                        None => return Err(callee.0 as usize),
                    }
                }
                ir::Expr::Shared(_) | ir::Expr::Repeat { max: Some(0), .. } => {}
                _ => todo.extend(expr.children()),
            }
        }
        Ok(size.min(MAX_INLINE_EXPRESSIONS + 1))
    }

    /// How `rule`, which lies on a left cycle, runs as a loop, if it can: where its body is a
    /// choice of recursive alternatives, each starting with a call of the rule itself, before
    /// base alternatives that cannot reach the rule where they start, and the rule cannot
    /// match nothing and holds no free cut. That is the growing loop of the semantics for
    /// this shape: in a pass with a seed every base alternative before the one that matched
    /// fails again, the recursive ones are tried in order on the seed, and the first that
    /// matches ends the pass, which grows only where it ends further. A pass in which none
    /// matches leaves the seed, since the base alternative that matched would end where the
    /// first seed ended.
    fn loop_of(&mut self, rule: RuleId, facts: &analysis::Facts) -> Option<Loop<'f>> {
        let rules: &'f [ir::Rule] = self.rules;
        let ir::Expr::Choice(alternatives) = &rules.get(rule.0 as usize)?.body else {
            return None;
        };
        let rests: Vec<&'f [ir::Expr]> = (alternatives.iter())
            .map_while(|alternative| after_call(alternative, rule))
            .collect();
        let (recursive, base) = alternatives.split_at(rests.len());
        let shaped = !recursive.is_empty() && !base.is_empty() && !facts.rule_nullable(rule);
        if !shaped
            || base
                .iter()
                .any(|alternative| facts.reaches_at_start(alternative, rule))
            || alternatives
                .iter()
                .any(|alternative| self.free_cut(alternative))
        {
            return None;
        }
        Some(Loop {
            recursive,
            rests,
            base,
        })
    }

    /// Compile a rule as the loop of [`Compiler::loop_of`]: `SeedOpen`, the base, and then
    /// `SeedKeep` and a choice of the recursive alternatives, each with `SeedUse` for its
    /// leading call, that grows the seed and goes round again where it ends further, until
    /// `SeedClose` gives the seed.
    fn left_loop(&mut self, rule: RuleId, looped: &Loop<'f>) {
        let Some((last, others)) = looped.recursive.split_last() else {
            return;
        };
        self.emit(Inst::SeedOpen);
        match looped.base {
            [one] => self.expr(one),
            _ => self.choice(looped.base),
        }
        let top = self.emit(Inst::SeedKeep) as u32;
        // The tests look at what follows the seed, where the recursive alternatives go on.
        let exit =
            (self.rests_test(&looped.rests)).map(|test| self.emit(Inst::Test { test, target: 0 }));
        let all = self.rests_bytes(&looped.rests);
        let choice = self.emit(Inst::Choice(0));
        let mut commits = Vec::new();
        for (alternative, rest) in others.iter().zip(&looped.rests) {
            let test = (self.rests_bytes(&[rest]) != all)
                .then(|| self.rests_test(&[rest]))
                .flatten()
                .map(|test| self.emit(Inst::Test { test, target: 0 }));
            let alt = self.emit(Inst::Choice(0));
            self.seed_rule = Some(rule);
            self.inside(true, alternative);
            commits.push(self.emit(Inst::Commit(0)));
            self.patch(alt);
            if let Some(test) = test {
                self.patch(test);
            }
        }
        let rest = looped.rests.last().copied().unwrap_or_default();
        if self.rests_bytes(&[rest]) != all
            && let Some(test) = self.rests_test(&[rest])
        {
            self.emit(Inst::TestFail(test));
        }
        self.seed_rule = Some(rule);
        self.inside(true, last);
        for commit in commits {
            self.patch(commit);
        }
        self.emit(Inst::SeedGrow(top));
        self.patch(choice);
        if let Some(exit) = exit {
            self.patch(exit);
        }
        self.emit(Inst::SeedClose);
    }

    /// The first bytes of code that goes on with one of these sequences of parts, where
    /// dispatch is on.
    fn rests_bytes(&self, rests: &[&[ir::Expr]]) -> Option<Bytes> {
        let starts = self.starts.as_ref()?;
        let union = |bytes: Bytes, parts: &&[ir::Expr]| bytes.union(starts.sequence(parts).0.bytes);
        Some(rests.iter().fold(Bytes::default(), union))
    }

    /// The test before code that goes on with one of these sequences of parts, tried in
    /// turn, if one is worth it.
    fn rests_test(&mut self, rests: &[&[ir::Expr]]) -> Option<u32> {
        let starts = self.starts.as_ref()?;
        let mut found: Option<(Start, Option<Vec<KindId>>)> = None;
        for parts in rests {
            let (start, heads) = starts.sequence(parts);
            found = Some(match found {
                Some((known, known_heads)) => (
                    known.merge(start),
                    analysis::joined(known_heads, heads.as_deref()),
                ),
                None => (start, heads),
            });
        }
        let (start, heads) = found?;
        self.test(start, heads, false, false)
    }

    /// The body that a call of `rule` compiles as, if it is inlined.
    fn inlined(&self, rule: RuleId) -> Option<&'f ir::Expr> {
        let rules: &'f [ir::Rule] = self.rules;
        (self.inline.get(rule.0 as usize) == Some(&true))
            .then(|| rules.get(rule.0 as usize).map(|r| &r.body))
            .flatten()
    }

    /// Whether the code of `expr` starts with an instruction that tests a byte, so that a
    /// test before it saves nothing.
    fn immediate(&self, expr: &ir::Expr) -> bool {
        match expr {
            ir::Expr::Literal { text, .. } => !text.is_empty(),
            ir::Expr::Class(_) | ir::Expr::Any => true,
            ir::Expr::Seq(parts) => parts.first().is_some_and(|part| self.immediate(part)),
            ir::Expr::Call(rule) => self.inlined(*rule).is_some_and(|body| self.immediate(body)),
            _ => false,
        }
    }
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
            | Inst::Test { target: t, .. }
            | Inst::RepOpen { end: t, .. } => *t = target,
            _ => {}
        }
    }

    /// Compile an expression. It recurses as deeply as the expression nests, which the
    /// resolver bounds.
    fn expr(&mut self, expr: &'f ir::Expr) {
        use ir::Expr as E;
        match expr {
            E::Empty => {}
            E::Literal {
                text,
                case_insensitive,
            } => self.literal(expr, text.as_bytes(), *case_insensitive),
            E::Class(set) => {
                let class = self.class(expr, set);
                self.emit(Inst::Class(class));
            }
            E::Any => {
                self.emit(Inst::Any);
            }
            E::Call(rule) if self.seed_rule == Some(*rule) => {
                self.seed_rule = None;
                self.emit(Inst::SeedUse);
            }
            // What stands around the call is not known to the body, as for a shared part.
            E::Call(rule) => match self.inlined(*rule) {
                Some(body) => self.inside(true, body),
                None => {
                    let at = self.emit(Inst::Call(0));
                    self.calls.push((at, *rule));
                }
            },
            E::Seq(parts) => {
                for part in parts {
                    self.expr(part);
                }
            }
            E::Choice(alternatives) => self.choice(alternatives),
            E::Repeat { expr, min, max } => self.repeat(expr, *min, *max),
            E::And(inner) => {
                let choice = self.emit(Inst::PredChoice(0));
                self.caught(false, inner, false);
                let back = self.emit(Inst::BackCommit(0));
                self.patch(choice);
                self.emit(Inst::Fail);
                self.patch(back);
            }
            E::Not(inner) => {
                let choice = self.emit(Inst::PredChoice(0));
                self.caught(false, inner, false);
                self.emit(Inst::FailTwice);
                self.patch(choice);
            }
            E::Cut => {
                self.emit(Inst::Cut);
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
                // A failure gives back what the expression captured before the skip that finds
                // the error's position, which may match a capture.
                let defines = self.defines(expr);
                if defines {
                    self.emit(Inst::CaptureMark);
                }
                self.expr(expr);
                if defines {
                    self.emit(Inst::CaptureKeep);
                }
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
                        self.queue.push((index, &**expr));
                        index
                    }
                };
                let at = self.emit(Inst::Sub(0));
                self.subs.push((at, index));
            }
            E::Indent { expr, skip } => {
                let skip = self.skip_before(*skip);
                self.emit(Inst::Indent { skip });
                self.expr(expr);
                self.emit(Inst::IndentEnd);
            }
            E::Same { skip } => {
                let skip = self.skip_before(*skip);
                self.emit(Inst::Same { skip });
            }
            E::Def {
                capture,
                expr,
                skip,
            } => {
                let skip = self.skip_before(*skip);
                self.emit(Inst::DefOpen { skip });
                self.expr(expr);
                self.emit(Inst::DefClose(capture.0));
            }
            E::Ref { capture } => {
                self.emit(Inst::Ref(capture.0));
            }
        }
    }

    /// Where a column check or a capture looks past ignored text first, the `Mark` and the skip
    /// before it, and whether it has them: the column is checked, or the text starts, where the
    /// skip ends, and what follows starts where it started, with the same skip, which the skip
    /// cache answers.
    fn skip_before(&mut self, skip: bool) -> bool {
        let skip = skip && self.has_skip;
        if skip {
            self.emit(Inst::Mark);
            self.emit(Inst::Skip);
        }
        skip
    }

    /// Compile `expr` where a cut can mark an entry of the routine, or cannot.
    fn inside(&mut self, cuttable: bool, expr: &'f ir::Expr) {
        let outer = std::mem::replace(&mut self.cuttable, cuttable);
        self.expr(expr);
        self.cuttable = outer;
    }

    /// As [`Compiler::inside`], for code whose failure an entry catches: where the code can make
    /// a capture, a failure gives it back, and a success keeps it where `keep` says so, which a
    /// lookahead does not.
    fn caught(&mut self, cuttable: bool, expr: &'f ir::Expr, keep: bool) {
        let defines = self.defines(expr);
        if defines {
            self.emit(Inst::CaptureMark);
        }
        self.inside(cuttable, expr);
        if defines {
            self.emit(if keep {
                Inst::CaptureKeep
            } else {
                Inst::CaptureDrop
            });
        }
    }

    /// Whether `expr` can make a capture in the activation it runs in: through a `%def` in it or
    /// in a shared part it calls. Not through a rule it calls, since a rule that makes one is
    /// never compiled into its callers. A shared part is looked at once.
    fn defines(&mut self, expr: &ir::Expr) -> bool {
        match expr {
            ir::Expr::Def { .. } => true,
            ir::Expr::Call(_) => false,
            ir::Expr::Shared(inner) => {
                let key = Arc::as_ptr(inner);
                if let Some(&known) = self.defines_shared.get(&key) {
                    return known;
                }
                let found = self.defines(inner);
                self.defines_shared.insert(key, found);
                found
            }
            _ => expr.children().iter().any(|child| self.defines(child)),
        }
    }

    /// Compile an expression that is a choice point of the semantics and has no entry of its
    /// own: the last alternative of a choice, an expression repeated exactly once, the skip
    /// expression. A cut in it must not mark an entry around it, so where it holds a free cut
    /// and there is such an entry, it gets a guard, a choice entry that resumes at a failure.
    fn cut_scope(&mut self, expr: &'f ir::Expr) {
        if !(self.cuttable && self.free_cut(expr)) {
            self.expr(expr);
            return;
        }
        let guard = self.emit(Inst::Choice(0));
        self.expr(expr);
        let commit = self.emit(Inst::Commit(0));
        self.patch(guard);
        self.emit(Inst::Fail);
        self.patch(commit);
    }

    /// Whether `expr` holds a cut that no choice, repetition, lookahead or call inside it
    /// takes. A shared part is looked at once.
    fn free_cut(&mut self, expr: &ir::Expr) -> bool {
        use ir::Expr as E;
        match expr {
            E::Cut => true,
            E::Seq(parts) => parts.iter().any(|part| self.free_cut(part)),
            E::Token { expr, .. }
            | E::Node { expr, .. }
            | E::Transparent { expr, .. }
            | E::Field { expr, .. }
            | E::Unreserved(expr)
            | E::FlagOn { expr, .. }
            | E::FlagOff { expr, .. }
            | E::Indent { expr, .. }
            | E::Def { expr, .. }
            | E::Label { expr, .. } => self.free_cut(expr),
            E::Shared(expr) => {
                let key = Arc::as_ptr(expr);
                if let Some(&known) = self.free_cuts.get(&key) {
                    return known;
                }
                let free = self.free_cut(expr);
                self.free_cuts.insert(key, free);
                free
            }
            E::Empty
            | E::Literal { .. }
            | E::Class(_)
            | E::Any
            | E::Call(_)
            | E::Choice(_)
            | E::Repeat { .. }
            | E::And(_)
            | E::Not(_)
            | E::Skip
            | E::WordEnd
            | E::FlagTest { .. }
            | E::Same { .. }
            | E::Ref { .. }
            | E::ErrorAlternative => false,
        }
    }

    /// Compile the literal `expr` of these bytes.
    fn literal(&mut self, expr: &ir::Expr, bytes: &[u8], case_insensitive: bool) {
        let folds = case_insensitive && bytes.iter().any(u8::is_ascii_alphabetic);
        match bytes {
            [] => {}
            [byte] if byte.is_ascii() && !folds => {
                self.emit(Inst::Char(*byte));
            }
            _ => {
                let key = std::ptr::from_ref(expr);
                let id = match self.strings_of.get(&key) {
                    Some(&id) => id,
                    None => {
                        let next = self.strings.len() as u32;
                        let id = *self.string_ids.entry(bytes.into()).or_insert(next);
                        if id == next {
                            self.strings.push(bytes.into());
                        }
                        self.strings_of.insert(key, id);
                        id
                    }
                };
                self.emit(if folds {
                    Inst::StrFold(id)
                } else {
                    Inst::Str(id)
                });
            }
        }
    }

    /// The test of the next byte before `expr`, if one is worth it: where `expr` cannot
    /// succeed without consuming input, does not start with every byte or with an instruction
    /// that tests the byte anyway, takes all its first bytes after a skip or all without one,
    /// and a failure before its first byte is a plain one. Where `expr` would try tokens
    /// there, the test records their failures, so it must surely try them, and with no rule
    /// in between that keeps the growing loop. With `pop`, a mismatch pops the entry of the
    /// loop the test is in.
    fn test_for(&mut self, expr: &ir::Expr, pop: bool) -> Option<u32> {
        let starts = self.starts.as_ref()?;
        let start = starts.of(expr);
        let heads = starts.heads(expr).map(<[KindId]>::to_vec);
        let immediate = self.immediate(expr);
        self.test(start, heads, immediate, pop)
    }

    /// The test before code with this start and these head kinds, as [`Compiler::test_for`]
    /// finds it, `immediate` where the code tests a byte at once.
    fn test(
        &mut self,
        start: Start,
        heads: Option<Vec<KindId>>,
        immediate: bool,
        pop: bool,
    ) -> Option<u32> {
        let after_skip = start.after_skip && self.has_skip;
        let worth = !start.nullable && start.bytes != Bytes::ALL && !immediate;
        if !worth || !start.plain || (after_skip && start.at_once) {
            return None;
        }
        let kinds: Box<[KindId]> = match start.tokens {
            false => Box::default(),
            true if start.clean && !start.growing => heads?.into(),
            true => return None,
        };
        let test = ByteTest {
            bytes: start.bytes,
            after_skip,
            pop,
            kinds,
        };
        let next = self.tests.len() as u32;
        let id = *self.test_ids.entry(test.clone()).or_insert(next);
        if id == next {
            self.tests.push(test);
        }
        Some(id)
    }

    fn choice(&mut self, alternatives: &'f [ir::Expr]) {
        let Some((last, rest)) = alternatives.split_last() else {
            self.emit(Inst::Fail);
            return;
        };
        // A test where an alternative starts with every byte the choice does fails only where
        // every alternative does, and in a left-recursive level, whose alternatives all start
        // with the level, only costs.
        let starts = self.starts.as_ref();
        let all = starts
            .map(|s| (alternatives.iter()).fold(Bytes::default(), |u, a| u.union(s.of(a).bytes)));
        let narrower = |c: &Self, alternative| {
            (c.starts.as_ref()).is_some_and(|s| Some(s.of(alternative).bytes) != all)
        };
        let mut commits = Vec::new();
        for alternative in rest {
            let test = (narrower(self, alternative))
                .then(|| self.test_for(alternative, false))
                .flatten()
                .map(|test| self.emit(Inst::Test { test, target: 0 }));
            let choice = self.emit(Inst::Choice(0));
            self.caught(true, alternative, true);
            commits.push(self.emit(Inst::Commit(0)));
            self.patch(choice);
            if let Some(test) = test {
                self.patch(test);
            }
        }
        if narrower(self, last)
            && let Some(test) = self.test_for(last, false)
        {
            self.emit(Inst::TestFail(test));
        }
        self.cut_scope(last);
        for commit in commits {
            self.patch(commit);
        }
    }

    /// The class of the first alternative of `expr` and the alternatives after it, where a
    /// loop over `expr` can split as `c* (e c*)*`: the first alternative is a class or one
    /// code point, maybe through a call compiled as its body, and no other holds a free cut.
    /// Both forms take `c` where it matches and try `e` where it does not, and a step of `e`
    /// that matches nothing ends both, since `c*` then matches nothing either.
    fn split(&mut self, expr: &'f ir::Expr) -> Option<(u32, &'f [ir::Expr])> {
        let ir::Expr::Choice(alternatives) = self.seen_through(expr) else {
            return None;
        };
        let (first, rest) = alternatives.split_first()?;
        let points = self.seen_through(first);
        let set = span_set(points)?;
        let free = rest.iter().any(|alternative| self.free_cut(alternative));
        (self.passes.split && !rest.is_empty() && !free).then(|| (self.class(points, &set), rest))
    }

    /// `expr`, or the body it compiles as where it is a call compiled into its caller.
    fn seen_through(&self, expr: &'f ir::Expr) -> &'f ir::Expr {
        match expr {
            ir::Expr::Call(rule) => {
                (self.inlined(*rule)).map_or(expr, |body| self.seen_through(body))
            }
            _ => expr,
        }
    }

    /// The class of `set`, the code points of the expression `points`, added to the program
    /// once for each such expression.
    fn class(&mut self, points: &ir::Expr, set: &ir::CharSet) -> u32 {
        let key = std::ptr::from_ref(points);
        if let Some(&class) = self.classes_of.get(&key) {
            return class;
        }
        self.classes.push(Class::new(set));
        let class = (self.classes.len() - 1) as u32;
        self.classes_of.insert(key, class);
        class
    }

    fn repeat(&mut self, expr: &'f ir::Expr, min: u32, max: Option<u32>) {
        if self.passes.span
            && max.is_none()
            && let Some((points, set, min)) = span(expr, min)
        {
            let class = self.class(points, &set);
            self.emit(Inst::Span { class, min });
            return;
        }
        match (min, max) {
            (0, Some(0)) => {}
            // The resolver rejects a maximum below the minimum.
            (_, Some(max)) if max < min => {
                self.emit(Inst::Fail);
            }
            (1, Some(1)) => self.cut_scope(expr),
            (0, Some(1)) => {
                let test = (self.test_for(expr, false))
                    .map(|test| self.emit(Inst::Test { test, target: 0 }));
                let choice = self.emit(Inst::Choice(0));
                self.caught(true, expr, true);
                let commit = self.emit(Inst::Commit(0));
                self.patch(choice);
                self.patch(commit);
                if let Some(test) = test {
                    self.patch(test);
                }
            }
            (0, None) if let Some((class, rest)) = self.split(expr) => {
                self.emit(Inst::Span { class, min: 0 });
                let choice = self.emit(Inst::Choice(0));
                let step = self.here();
                let test = match rest {
                    [one] => (self.test_for(one, true))
                        .map(|test| self.emit(Inst::Test { test, target: 0 })),
                    _ => None,
                };
                let defines = rest.iter().any(|alternative| self.defines(alternative));
                if defines {
                    self.emit(Inst::CaptureMark);
                }
                match rest {
                    [one] => self.inside(true, one),
                    _ => self.inside_choice(rest),
                }
                if defines {
                    self.emit(Inst::CaptureKeep);
                }
                self.emit(Inst::Span { class, min: 0 });
                self.emit(Inst::LoopCommit(step));
                self.patch(choice);
                if let Some(test) = test {
                    self.patch(test);
                }
            }
            (0, None) => {
                let choice = self.emit(Inst::Choice(0));
                let step = self.here();
                let test = (self.test_for(expr, true))
                    .map(|test| self.emit(Inst::Test { test, target: 0 }));
                self.caught(true, expr, true);
                self.emit(Inst::LoopCommit(step));
                self.patch(choice);
                if let Some(test) = test {
                    self.patch(test);
                }
            }
            (min, max) => {
                self.bounds.push((min, max.unwrap_or(u32::MAX)));
                let bounds = (self.bounds.len() - 1) as u32;
                let open = self.emit(Inst::RepOpen { end: 0, bounds });
                let step = self.here();
                self.caught(true, expr, true);
                self.emit(Inst::RepNext(step));
                self.patch(open);
            }
        }
    }

    /// The alternatives as a choice where a cut can mark an entry of the routine.
    fn inside_choice(&mut self, alternatives: &'f [ir::Expr]) {
        let outer = std::mem::replace(&mut self.cuttable, true);
        self.choice(alternatives);
        self.cuttable = outer;
    }

    /// `Open`, the expression, and the instruction that closes what it collected.
    fn collect(&mut self, expr: &'f ir::Expr, close: Inst) {
        self.emit(Inst::Open);
        self.expr(expr);
        self.emit(close);
    }
}
