// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Checking a grammar: its errors, and warnings about what works and likely not as meant.
//!
//! A warning says only what holds for every input. Whether an alternative can ever match is
//! undecidable in general, so each check is a named case that can be told from the grammar as
//! written, and what it cannot tell it leaves alone. One takes something for granted that a
//! grammar can make untrue, which its page says: W301, that every part can match as written.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::diag::{Diagnostic, DiagnosticCode};
use crate::grammar::{
    ClassItem, ErrorLabel, Expr, ExprKind, Grammar, Name, RuleBody, RuleKind, Span,
};
use crate::ir::{self, CharSet};
use crate::items::Counts;
use crate::vm::{Options, compile};

/// Alternatives before an alternative that it is compared with. A choice between more is rare,
/// and comparing all pairs of a very long one would take time that grows with its square.
const MAX_COMPARED_ALTERNATIVES: usize = 256;

/// Everything to say about a grammar. If it does not resolve, the errors of that. Otherwise a
/// grammar too large to compile, which is an error too, and the warnings. Sorted by where they
/// stand in the grammar.
pub fn check(grammar: &Grammar) -> Vec<Diagnostic> {
    let grammar = match crate::grammar::link::link_library(grammar) {
        Ok(grammar) => grammar,
        Err(errors) => return errors,
    };
    // Placed at their directives, the problems of modules move ahead of later ones.
    let mut found = grammar.locate(check_joined(&grammar));
    found.sort_by_key(|diagnostic| (diagnostic.span.start, diagnostic.span.end));
    found
}

/// The problems of a grammar joined with its modules.
fn check_joined(grammar: &Grammar) -> Vec<Diagnostic> {
    let (resolved, counts) = match crate::resolve::resolve_counted(grammar) {
        Ok(resolved) => resolved,
        Err(errors) => return errors,
    };
    let mut found = compile::compile(&resolved, &Options::default())
        .err()
        .unwrap_or_default();
    shapes(grammar, &resolved, &counts, &mut found);
    let bodies = bodies(grammar);
    unused(grammar, &bodies, &mut found);
    let mut checker = Checker {
        word: word_set(grammar),
        boundaries: false,
        empty: Empties::of(grammar, &bodies),
        found: Vec::new(),
    };
    // An error of the skip expression comes before a token, also where a step would match
    // nothing, so only the repetitions of terminals, which do not skip, are left to tell.
    let raising = &checker.empty.raising;
    let skip_raises = (grammar.ignore.iter()).any(|expr| may_raise(expr, &[], raising));
    for body in &bodies {
        // Only the literals of rules that make nodes end at a word boundary.
        checker.boundaries = checker.word.is_some() && !body.terminal;
        checker.alternatives(&body.alternatives, body.params);
        // A level ends with the step to the next one, as its last alternative.
        if let Some(next) = body.next
            && (body.alternatives.last()).is_some_and(|last| never_fails(last, body.params))
        {
            let message = format!(
                "the level {} is never reached from {}, whose last alternative cannot fail",
                next.text, body.name
            );
            checker.warn(DiagnosticCode::UnreachableAlternative, message, next.span);
        }
        for alternative in &body.alternatives {
            checker.recursion(body.name, body.params, alternative);
            if body.terminal || !skip_raises {
                checker.repetitions(alternative);
            }
        }
    }
    checker.boundaries = false;
    // The skip expression is the ordered choice of the `%ignore` expressions.
    checker.alternatives(&grammar.ignore.iter().collect::<Vec<_>>(), &[]);
    for expr in &grammar.ignore {
        checker.repetitions(expr);
    }
    found.append(&mut checker.found);
    found.sort_by_key(|diagnostic| (diagnostic.span.start, diagnostic.span.end));
    found.dedup();
    found
}

/// Warn about the alternatives of transparent rules that make a node for some inputs only,
/// and about builders on those that never make one. A rule with parameters is left alone, since
/// what its alternatives leave depends on its arguments.
fn shapes(grammar: &Grammar, resolved: &ir::Grammar, counts: &Counts, found: &mut Vec<Diagnostic>) {
    let lowered: HashMap<&str, &ir::Rule> = (resolved.rules.iter())
        .map(|rule| (rule.name.as_str(), rule))
        .collect();
    for rule in &grammar.rules {
        let name = &rule.name.text;
        let (RuleBody::Alternatives(alternatives), Some(lowered)) =
            (&rule.body, lowered.get(name.as_str()))
        else {
            continue;
        };
        if !rule.transparent || !rule.params.is_empty() {
            continue;
        }
        // The body of a rule is its one alternative, or the choice between several.
        let nodes = match &lowered.body {
            ir::Expr::Choice(nodes) => nodes.as_slice(),
            node => std::slice::from_ref(node),
        };
        for (alternative, node) in alternatives.iter().zip(nodes) {
            // An alternative with an alias always makes a node.
            let ir::Expr::Transparent { expr, .. } = node else {
                continue;
            };
            let shape = counts.shape(expr);
            let (code, message) = if shape.stays && shape.gives_way {
                let message = format!(
                    "this alternative makes a node {name} for some inputs and gives way to its \
                     one child for others"
                );
                (DiagnosticCode::UnsteadyShape, message)
            } else if shape.gives_way && alternative.builder.is_some() {
                let message = format!(
                    "the builder of this alternative is never used, since it always gives way to \
                     its one child and makes no node {name}"
                );
                (DiagnosticCode::UnusedBuilder, message)
            } else {
                continue;
            };
            found.push(Diagnostic {
                code,
                message,
                span: alternative.expr.span,
            });
        }
    }
}

/// The names that expressions refer to, the flags that they set and test, and the labels
/// that they raise.
#[derive(Default)]
struct Uses<'g> {
    names: HashSet<&'g str>,
    set: HashSet<&'g str>,
    tested: HashSet<&'g str>,
    labels: HashSet<&'g str>,
}

impl<'g> Uses<'g> {
    fn add(&mut self, exprs: &[&'g Expr]) {
        let mut pending = exprs.to_vec();
        while let Some(expr) = pending.pop() {
            match &expr.kind {
                ExprKind::Reference { name, .. } => self.names.insert(&name.text),
                ExprKind::FlagOn { flag, .. } => self.set.insert(&flag.text),
                ExprKind::FlagTest { flag, .. } => self.tested.insert(&flag.text),
                ExprKind::Label {
                    label: ErrorLabel::Named(name),
                    ..
                } => self.labels.insert(&name.text),
                _ => false,
            };
            pending.extend(expr.children());
        }
    }
}

/// Warn about the parameters that their rules never use, the flags that are never set or
/// never tested, and the recoveries of labels that nothing raises.
fn unused(grammar: &Grammar, bodies: &[Body<'_>], found: &mut Vec<Diagnostic>) {
    let mut uses = Uses::default();
    for body in bodies {
        uses.names.clear();
        uses.add(&body.alternatives);
        for param in body.params {
            if !uses.names.contains(param.text.as_str()) {
                let message = format!("{} never uses its parameter {}", body.name, param.text);
                found.push(Diagnostic {
                    code: DiagnosticCode::UnusedParameter,
                    message,
                    span: param.span,
                });
            }
        }
    }
    // The ignored text is matched with the flags of the place it is skipped at, and so is a
    // recovery with those of the label.
    uses.add(&grammar.ignore.iter().collect::<Vec<_>>());
    uses.add(&grammar.recovers.iter().map(|r| &r.expr).collect::<Vec<_>>());
    for flag in &grammar.flags {
        let name = flag.text.as_str();
        let message = match (uses.set.contains(name), uses.tested.contains(name)) {
            (true, true) => continue,
            (true, false) => format!("the flag {name} is set with %on and never tested with %if"),
            (false, true) => format!(
                "the flag {name} is never set with %on, so every test of it gives the same answer"
            ),
            (false, false) => format!("the flag {name} is never set and never tested"),
        };
        found.push(Diagnostic {
            code: DiagnosticCode::UnusedFlag,
            message,
            span: flag.span,
        });
    }
    // A grammar that replaces the rules raising a label of a module it extends leaves that
    // module's recovery unused, which is no mistake of its own.
    for recover in &grammar.recovers {
        let label = recover.label.text.as_str();
        if !uses.labels.contains(label) && !grammar.in_module(recover.label.span) {
            found.push(Diagnostic {
                code: DiagnosticCode::UnusedRecovery,
                message: format!(
                    "no expression raises the label {label}, so its recovery never runs"
                ),
                span: recover.label.span,
            });
        }
    }
}

/// A rule, or a level of a precedence block: what a name stands for.
struct Body<'g> {
    name: &'g str,
    params: &'g [Name],
    terminal: bool,
    alternatives: Vec<&'g Expr>,
    /// The level that this level falls through to.
    next: Option<&'g Name>,
}

fn bodies(grammar: &Grammar) -> Vec<Body<'_>> {
    let mut bodies = Vec::new();
    for rule in &grammar.rules {
        let terminal = rule.kind() == RuleKind::Terminal;
        match &rule.body {
            RuleBody::Alternatives(alternatives) => bodies.push(Body {
                name: &rule.name.text,
                params: &rule.params,
                terminal,
                alternatives: alternatives.iter().map(|a| &a.expr).collect(),
                next: None,
            }),
            RuleBody::Precedence(levels) => {
                for (index, level) in levels.iter().enumerate() {
                    bodies.push(Body {
                        name: &level.name.text,
                        params: &[],
                        terminal,
                        alternatives: level.alternatives.iter().map(|a| &a.expr).collect(),
                        next: levels.get(index + 1).map(|next| &next.name),
                    });
                }
            }
        }
    }
    bodies
}

/// The `need` of an expression that is not known to match the empty string.
const NEVER: u32 = u32::MAX;

/// Which expressions match the empty string at the end of every input, where nothing consumes:
/// those with a way through them of optional parts alone. What looks ahead or depends on flags,
/// columns, captures or parameters may fail there, and is not among them. Nor is what lies
/// behind a part that can fail abruptly, which may end the choice or the repetition first.
///
/// Found as `analysis` finds what can succeed without consuming input, in time linear in the
/// size of the grammar: an expression is counted down once for each part that matches the empty
/// string, until it needs no more.
struct Empties<'g> {
    /// The numbers of the rules and levels, which come first.
    names: HashMap<&'g str, u32>,
    /// The number of every expression, by its address.
    numbers: HashMap<*const Expr, u32>,
    /// How many more of its parts must match the empty string for each to, or [`NEVER`].
    need: Vec<u32>,
    /// The expressions that each one is such a part of.
    wholes: Vec<Vec<u32>>,
    /// The rules and levels that can end the parse with an error.
    raising: HashSet<&'g str>,
}

impl<'g> Empties<'g> {
    fn of(grammar: &'g Grammar, bodies: &[Body<'g>]) -> Self {
        let mut names: HashMap<&str, u32> = (bodies.iter().zip(0..))
            .map(|(body, number)| (body.name, number))
            .collect();
        // A precedence block calls its first level.
        for rule in &grammar.rules {
            if let RuleBody::Precedence(levels) = &rule.body
                && let Some(level) = levels.first()
                && let Some(&first) = names.get(level.name.text.as_str())
            {
                names.insert(&rule.name.text, first);
            }
        }
        let mut empties = Empties {
            names,
            numbers: HashMap::new(),
            // A rule matches the empty string once one of its alternatives does.
            need: vec![1; bodies.len()],
            wholes: vec![Vec::new(); bodies.len()],
            raising: raising(grammar, bodies),
        };
        for (body, number) in bodies.iter().zip(0..) {
            let closed = closing(&body.alternatives, body.params, &empties.raising);
            for (index, alternative) in body.alternatives.iter().enumerate() {
                let part = empties.number(alternative, body.params);
                if closed.is_none_or(|last| index <= last) {
                    empties.wholes[part as usize].push(number);
                }
            }
            // A level ends with the step to the next one, as its last alternative.
            if closed.is_none()
                && let Some(next) = body.next
                && let Some(&next) = empties.names.get(next.text.as_str())
            {
                empties.wholes[next as usize].push(number);
            }
        }
        for expr in &grammar.ignore {
            empties.number(expr, &[]);
        }
        empties.settle();
        empties
    }

    /// Number an expression and the expressions in it, and note what each needs. It recurses
    /// as deeply as the expression nests, which the resolver bounds.
    fn number(&mut self, expr: &Expr, params: &[Name]) -> u32 {
        let number = self.need.len() as u32;
        self.need.push(NEVER);
        self.wholes.push(Vec::new());
        self.numbers.insert(expr, number);
        // An expression that does not count, such as an argument, is asked about all the same.
        let inner = expr.children();
        let parts: Vec<u32> = (inner.iter())
            .map(|part| self.number(part, params))
            .collect();
        // What the expression needs, and how many of its first parts count towards that.
        let (need, counted) = match &expr.kind {
            ExprKind::Empty | ExprKind::Cut => (0, 0),
            ExprKind::Reference { name, .. } => {
                // A parameter stands for any expression, also under the name of a rule.
                let bound = params.iter().any(|param| param.text == name.text);
                match self.names.get(name.text.as_str()) {
                    Some(&rule) if !bound => {
                        self.wholes[rule as usize].push(number);
                        (1, 0)
                    }
                    _ => (NEVER, 0),
                }
            }
            ExprKind::Sequence(_) => (parts.len() as u32, parts.len()),
            ExprKind::Choice(_) => {
                let counted =
                    closing(&inner, params, &self.raising).map_or(parts.len(), |last| last + 1);
                (1, counted)
            }
            ExprKind::Repeat { max: Some(0), .. } => (0, 0),
            // No step is made where the expression fails, unless it fails abruptly.
            ExprKind::Repeat {
                expr: step, min: 0, ..
            } => match abrupt(step, params, &self.raising) {
                false => (0, 0),
                true => (1, 1),
            },
            // `**` makes no step where its first item fails, and one item that matches nothing
            // is a match of `++`, unless what comes after it fails abruptly.
            ExprKind::Separated {
                item,
                separator,
                at_least_one,
            } => match (
                abrupt(separator, params, &self.raising),
                *at_least_one || abrupt(item, params, &self.raising),
            ) {
                (true, _) => (NEVER, 0),
                (false, true) => (1, 1),
                (false, false) => (0, 0),
            },
            ExprKind::Repeat { .. }
            | ExprKind::Field { .. }
            | ExprKind::Label { .. }
            | ExprKind::FlagOn { .. }
            | ExprKind::FlagOff { .. }
            | ExprKind::Def { .. } => (1, 1),
            // What consumes, and what cannot be told.
            _ => (NEVER, 0),
        };
        self.need[number as usize] = need;
        for &part in parts.iter().take(counted) {
            self.wholes[part as usize].push(number);
        }
        number
    }

    /// Count every expression down once for each part that matches the empty string.
    fn settle(&mut self) {
        let mut ready: Vec<u32> = (0..self.need.len() as u32)
            .filter(|&number| self.need[number as usize] == 0)
            .collect();
        while let Some(number) = ready.pop() {
            for &whole in &self.wholes[number as usize] {
                let count = &mut self.need[whole as usize];
                if *count != 0 && *count != NEVER {
                    *count -= 1;
                    if *count == 0 {
                        ready.push(whole);
                    }
                }
            }
        }
    }

    /// Whether the expression matches the empty string at the end of every input.
    fn holds(&self, expr: &Expr) -> bool {
        let number = self.numbers.get(&std::ptr::from_ref(expr));
        number.is_some_and(|&number| self.need[number as usize] == 0)
    }
}

/// The word characters of a grammar with `%word`.
fn word_set(grammar: &Grammar) -> Option<CharSet> {
    match &grammar.word.as_ref()?.kind {
        ExprKind::Class(class) => Some(CharSet {
            negated: class.negated,
            items: class.items.clone(),
        }),
        _ => None,
    }
}

/// The expression without the fields around it, which do not change what it matches.
fn bare(expr: &Expr) -> &Expr {
    match &expr.kind {
        ExprKind::Field { expr, .. } => bare(expr),
        _ => expr,
    }
}

/// The parts of an alternative in turn.
fn parts(expr: &Expr) -> Vec<&Expr> {
    match &bare(expr).kind {
        ExprKind::Sequence(items) => items.iter().collect(),
        _ => vec![expr],
    }
}

/// Whether two expressions are written alike, apart from fields: such expressions match the
/// same at the same place.
fn alike(a: &Expr, b: &Expr) -> bool {
    use ExprKind as K;
    let (a, b) = (bare(a), bare(b));
    let same = match (&a.kind, &b.kind) {
        (K::Reference { name: x, .. }, K::Reference { name: y, .. }) => x.text == y.text,
        (
            K::Literal {
                text: x,
                case_insensitive: i,
            },
            K::Literal {
                text: y,
                case_insensitive: j,
            },
        ) => x == y && i == j,
        (K::Class(x), K::Class(y)) => x == y,
        (K::Repeat { min: x, max: i, .. }, K::Repeat { min: y, max: j, .. }) => x == y && i == j,
        (
            K::Separated {
                at_least_one: x, ..
            },
            K::Separated {
                at_least_one: y, ..
            },
        ) => x == y,
        (K::FlagOn { flag: x, .. }, K::FlagOn { flag: y, .. })
        | (K::FlagOff { flag: x, .. }, K::FlagOff { flag: y, .. })
        | (K::Def { name: x, .. }, K::Def { name: y, .. })
        | (K::Ref { name: x }, K::Ref { name: y }) => x.text == y.text,
        (
            K::FlagTest {
                flag: x,
                negated: i,
            },
            K::FlagTest {
                flag: y,
                negated: j,
            },
        ) => x.text == y.text && i == j,
        (K::Label { label: x, .. }, K::Label { label: y, .. }) => label_text(x) == label_text(y),
        (K::Error { message: x }, K::Error { message: y }) => x.value == y.value,
        (x, y) => std::mem::discriminant(x) == std::mem::discriminant(y),
    };
    let (inner_a, inner_b) = (a.children(), b.children());
    same && inner_a.len() == inner_b.len() && inner_a.iter().zip(&inner_b).all(|(x, y)| alike(x, y))
}

/// A hash of how an expression is written, apart from fields. Expressions that are [`alike`]
/// share it, so that most pairs that are not are told apart without comparing them.
fn written(expr: &Expr) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_written(expr, &mut hasher);
    hasher.finish()
}

/// Hash what [`alike`] compares. It recurses as deeply as the expression nests, which the
/// resolver bounds.
fn hash_written(expr: &Expr, hasher: &mut DefaultHasher) {
    use ExprKind as K;
    let expr = bare(expr);
    std::mem::discriminant(&expr.kind).hash(hasher);
    match &expr.kind {
        K::Reference { name, .. }
        | K::FlagOn { flag: name, .. }
        | K::FlagOff { flag: name, .. }
        | K::Def { name, .. }
        | K::Ref { name } => name.text.hash(hasher),
        K::Literal {
            text,
            case_insensitive,
        } => (text, case_insensitive).hash(hasher),
        K::Repeat { min, max, .. } => (min, max).hash(hasher),
        K::Separated { at_least_one, .. } => at_least_one.hash(hasher),
        K::FlagTest { flag, negated } => (&flag.text, negated).hash(hasher),
        K::Label { label, .. } => label_text(label).hash(hasher),
        K::Error { message } => message.value.hash(hasher),
        K::Class(class) => {
            class.negated.hash(hasher);
            for item in &class.items {
                std::mem::discriminant(item).hash(hasher);
                match item {
                    ClassItem::Range(first, last) => (first, last).hash(hasher),
                    ClassItem::Property { property, negated } => {
                        (std::mem::discriminant(property), negated).hash(hasher);
                    }
                }
            }
        }
        _ => {}
    }
    let children = expr.children();
    children.len().hash(hasher);
    for child in children {
        hash_written(child, hasher);
    }
}

/// What an error label says, by message or by name, wherever it is written.
fn label_text(label: &ErrorLabel) -> (bool, &str) {
    match label {
        ErrorLabel::Message(message) => (false, &message.value),
        ErrorLabel::Named(name) => (true, &name.text),
    }
}

/// Whether the expression matches wherever it is tried, or ends the parse with an error, so
/// that no alternative after it is ever tried. Rules and parameters are not looked into.
fn never_fails(expr: &Expr, params: &[Name]) -> bool {
    match &expr.kind {
        ExprKind::Empty | ExprKind::Cut => true,
        ExprKind::Sequence(items) => items.iter().all(|item| never_fails(item, params)),
        // A failure after a cut ends the choice, so only the alternatives up to the first that
        // can cut are sure to be tried.
        ExprKind::Choice(items) => {
            for item in items {
                if never_fails(item, params) {
                    return true;
                }
                if may_cut(item, params) {
                    return false;
                }
            }
            false
        }
        ExprKind::Repeat { max: Some(0), .. } => true,
        // A failure of the expression after a cut makes the repetition fail.
        ExprKind::Repeat {
            expr: step, min: 0, ..
        } => !may_cut(step, params) || never_fails(step, params),
        ExprKind::Repeat { expr: step, .. } => never_fails(step, params),
        // `**` fails only where its first item passes a cut and then fails.
        ExprKind::Separated {
            item,
            at_least_one: false,
            ..
        } => !may_cut(item, params),
        // A labeled expression that does not match is an error, not a failure.
        ExprKind::Label { .. } => true,
        ExprKind::Field { expr, .. }
        | ExprKind::FlagOn { expr, .. }
        | ExprKind::FlagOff { expr, .. }
        | ExprKind::Def { expr, .. } => never_fails(expr, params),
        _ => false,
    }
}

fn is_param(name: &Name, params: &[Name]) -> bool {
    params.iter().any(|param| param.text == name.text)
}

/// Whether a cut can reach the choice point around the expression. A choice, a repetition, a
/// lookahead and a rule keep the cuts inside them, a label does not. An argument can hold a
/// cut, so a parameter counts as one that can.
fn may_cut(expr: &Expr, params: &[Name]) -> bool {
    match &expr.kind {
        ExprKind::Cut => true,
        ExprKind::Reference { name, .. } => is_param(name, params),
        // The first item of `++` is matched outside the repetition of the others.
        ExprKind::Separated {
            item,
            at_least_one: true,
            ..
        } => may_cut(item, params),
        ExprKind::Choice(_)
        | ExprKind::Repeat { .. }
        | ExprKind::Separated { .. }
        | ExprKind::And(_)
        | ExprKind::Not(_) => false,
        _ => (expr.children().iter()).any(|child| may_cut(child, params)),
    }
}

/// Whether the expression can end the parse with an error: it holds an error label outside a
/// lookahead, which catches errors, or calls a rule there that can. An argument can hold a
/// label.
fn may_raise(expr: &Expr, params: &[Name], raising: &HashSet<&str>) -> bool {
    match &expr.kind {
        ExprKind::Label { .. } => true,
        ExprKind::Reference { name, .. } => {
            is_param(name, params) || raising.contains(name.text.as_str())
        }
        ExprKind::And(_) | ExprKind::Not(_) => false,
        _ => (expr.children().iter()).any(|child| may_raise(child, params, raising)),
    }
}

/// The rules and levels that can end the parse with an error: those with an error label or a
/// parameter outside a lookahead, and those that call one of them there, a level also by
/// falling through to the next. Found by walking each body once and passing what raises back
/// to its callers, in time linear in the size of the grammar.
fn raising<'g>(grammar: &'g Grammar, bodies: &[Body<'g>]) -> HashSet<&'g str> {
    let mut callers: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut ready = Vec::new();
    for body in bodies {
        let mut calls = Vec::new();
        let labeled = (body.alternatives.iter())
            .any(|alternative| labels(alternative, body.params, &mut calls));
        if labeled {
            ready.push(body.name);
        }
        calls.extend(body.next.map(|next| next.text.as_str()));
        for called in calls {
            callers.entry(called).or_default().push(body.name);
        }
    }
    // A precedence block calls its first level.
    for rule in &grammar.rules {
        if let RuleBody::Precedence(levels) = &rule.body
            && let Some(first) = levels.first()
        {
            let callers = callers.entry(first.name.text.as_str()).or_default();
            callers.push(&rule.name.text);
        }
    }
    let mut raising = HashSet::new();
    while let Some(name) = ready.pop() {
        if raising.insert(name) {
            ready.extend(callers.get(name).into_iter().flatten().copied());
        }
    }
    raising
}

/// Whether the expression holds an error label or a parameter outside a lookahead. The names it
/// calls there are noted until one is found.
fn labels<'g>(expr: &'g Expr, params: &[Name], calls: &mut Vec<&'g str>) -> bool {
    match &expr.kind {
        ExprKind::Label { .. } => true,
        ExprKind::Reference { name, .. } if is_param(name, params) => true,
        ExprKind::Reference { name, .. } => {
            calls.push(&name.text);
            false
        }
        ExprKind::And(_) | ExprKind::Not(_) => false,
        _ => (expr.children().into_iter()).any(|child| labels(child, params, calls)),
    }
}

/// Whether the expression can fail abruptly: after a cut, which ends the choice or the
/// repetition around it, or with an error, which ends the parse.
fn abrupt(expr: &Expr, params: &[Name], raising: &HashSet<&str>) -> bool {
    may_cut(expr, params) || may_raise(expr, params, raising)
}

/// The place of the first alternative after which no other is sure to be tried: one that
/// cannot fail, or one that can fail abruptly.
fn closing(alternatives: &[&Expr], params: &[Name], raising: &HashSet<&str>) -> Option<usize> {
    (alternatives.iter()).position(|alternative| {
        never_fails(alternative, params) || abrupt(alternative, params, raising)
    })
}

struct Checker<'g> {
    /// The word characters, if the grammar sets them.
    word: Option<CharSet>,
    /// Whether a literal that ends in a word character fails before another one.
    boundaries: bool,
    /// The expressions that match the empty string.
    empty: Empties<'g>,
    found: Vec<Diagnostic>,
}

impl Checker<'_> {
    fn warn(&mut self, code: DiagnosticCode, message: String, span: Span) {
        self.found.push(Diagnostic {
            code,
            message,
            span,
        });
    }

    /// The two literals, if `later` can never match after `earlier` in a choice, since the
    /// earlier one matches wherever the later one would.
    fn shadowed(&self, earlier: &Expr, later: &Expr) -> Option<(String, String)> {
        let (
            ExprKind::Literal {
                text: first,
                case_insensitive: first_folds,
            },
            ExprKind::Literal {
                text: second,
                case_insensitive: second_folds,
            },
        ) = (&bare(earlier).kind, &bare(later).kind)
        else {
            return None;
        };
        let start = second.get(..first.len())?;
        let starts_with = match (first_folds, second_folds) {
            (false, false) => start == first,
            (true, _) => start.eq_ignore_ascii_case(first),
            // The later one also matches spellings that the earlier one does not.
            (false, true) => false,
        };
        let rest = second.get(first.len()..)?;
        // Written alike they are duplicates, which the check of whole alternatives reports.
        if !starts_with || (rest.is_empty() && first_folds == second_folds && start == first) {
            return None;
        }
        // At a word boundary the shorter literal fails where the longer one goes on with a
        // word character, in either case where the longer one ignores case. A literal gets its
        // boundary by its last character as written, so a later one as long fails nowhere the
        // earlier one does where its last character is no word character.
        if self.boundaries
            && let (Some(word), Some(last)) = (&self.word, first.chars().last())
            && word.contains(last)
        {
            let gives_way = match rest.chars().next() {
                Some(next) => {
                    word.contains(next)
                        || (*second_folds
                            && (word.contains(next.to_ascii_lowercase())
                                || word.contains(next.to_ascii_uppercase())))
                }
                None => second.chars().last().is_some_and(|end| !word.contains(end)),
            };
            if gives_way {
                return None;
            }
        }
        Some((first.clone(), second.clone()))
    }

    /// The checks of an ordered choice between `alternatives`, in a rule with `params`.
    fn alternatives(&mut self, alternatives: &[&Expr], params: &[Name]) {
        // The parts of every alternative and how each is written, made once for all pairs.
        let split: Vec<(Vec<&Expr>, Vec<u64>)> = (alternatives.iter())
            .map(|alternative| {
                let parts = parts(alternative);
                let keys = parts.iter().map(|part| written(part)).collect();
                (parts, keys)
            })
            .collect();
        let numbered = alternatives.iter().zip(&split).enumerate();
        for (index, (later, (whole, whole_keys))) in numbered.skip(1) {
            let before = alternatives.get(..index).unwrap_or_default();
            if before
                .last()
                .is_some_and(|earlier| never_fails(earlier, params))
            {
                let message = "this alternative is never tried, since the one before it cannot \
                               fail";
                let code = DiagnosticCode::UnreachableAlternative;
                self.warn(code, message.to_owned(), later.span);
            }
            let compared = before.len().saturating_sub(MAX_COMPARED_ALTERNATIVES);
            let pairs = before.iter().zip(&split).skip(compared);
            for (earlier, (start, start_keys)) in pairs {
                if let Some((first, second)) = self.shadowed(earlier, later) {
                    let message = format!(
                        "{second:?} never matches here, since {first:?} before it matches first"
                    );
                    self.warn(DiagnosticCode::ShadowedLiteral, message, later.span);
                    break;
                }
                if whole_keys.starts_with(start_keys)
                    && start.iter().zip(whole).all(|(a, b)| alike(a, b))
                {
                    let message = "this alternative never matches, since it starts with the whole \
                                   of an earlier one";
                    let code = DiagnosticCode::ShadowedAlternative;
                    self.warn(code, message.to_owned(), later.span);
                    break;
                }
            }
        }
        for alternative in alternatives {
            self.inside(alternative, params);
        }
    }

    /// Warn about the repetitions in an expression that can end at a step that matches
    /// nothing.
    fn repetitions(&mut self, expr: &Expr) {
        let stops = match &expr.kind {
            ExprKind::Repeat { expr, max, .. } if max.is_none_or(|max| max > 1) => {
                self.empty.holds(expr)
            }
            // A step is a separator and an item.
            ExprKind::Separated {
                item, separator, ..
            } => self.empty.holds(item) && self.empty.holds(separator),
            _ => false,
        };
        if stops {
            let message = "this repetition ends at the first step that matches nothing, which \
                           its expression can";
            self.warn(
                DiagnosticCode::EmptyRepetition,
                message.to_owned(),
                expr.span,
            );
        }
        for child in expr.children() {
            self.repetitions(child);
        }
    }

    /// Warn about an alternative of `name` that begins and ends with `name` itself, called
    /// with its own parameters if it has any. In a template a bare name that calls nothing is
    /// a parameter, also one named like the rule.
    fn recursion(&mut self, name: &str, params: &[Name], alternative: &Expr) {
        let own = |arg: &Expr, param: &Name| {
            matches!(&bare(arg).kind, ExprKind::Reference { name, args }
                if name.text == param.text && args.is_empty())
        };
        let calls = |part: &&Expr| {
            matches!(&bare(part).kind, ExprKind::Reference { name: called, args }
                if called.text == name
                    && args.len() == params.len()
                    && args.iter().zip(params).all(|(arg, param)| own(arg, param)))
        };
        let parts = parts(alternative);
        if parts.len() > 1 && parts.first().is_some_and(calls) && parts.last().is_some_and(calls) {
            let message = format!(
                "{name} calls itself at both ends of this alternative, which groups to the \
                 right"
            );
            let code = DiagnosticCode::LeftAndRightRecursion;
            self.warn(code, message, alternative.span);
        }
    }

    /// The checks of the choices inside an expression.
    fn inside(&mut self, expr: &Expr, params: &[Name]) {
        match &expr.kind {
            ExprKind::Choice(items) => {
                self.alternatives(&items.iter().collect::<Vec<_>>(), params);
            }
            _ => {
                for child in expr.children() {
                    self.inside(child, params);
                }
            }
        }
    }
}
