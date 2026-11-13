// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Checking a grammar: its errors, and warnings about what works and likely not as meant.
//!
//! A warning says only what holds for every input. Whether an alternative can ever match is
//! undecidable in general, so each check is a named case that can be told from the grammar as
//! written, and what it cannot tell it leaves alone.

use std::collections::HashMap;

use crate::diag::{Diagnostic, DiagnosticCode};
use crate::grammar::{Expr, ExprKind, Grammar, Name, RuleBody, RuleKind, Span};
use crate::ir::CharSet;
use crate::vm::{Options, compile};

/// Alternatives before an alternative that it is compared with. A choice between more is rare,
/// and comparing all pairs of a very long one would take time that grows with its square.
const MAX_COMPARED_ALTERNATIVES: usize = 256;

/// Everything to say about a grammar. If it does not resolve, the errors of that. Otherwise the
/// forms that the parser does not run yet, which are errors too, and the warnings. Sorted by
/// where they stand in the grammar.
pub fn check(grammar: &Grammar) -> Vec<Diagnostic> {
    let resolved = match crate::resolve::resolve(grammar) {
        Ok(resolved) => resolved,
        Err(errors) => return errors,
    };
    let mut found = compile::compile(&resolved, &Options::default())
        .err()
        .unwrap_or_default();
    let bodies = bodies(grammar);
    let mut checker = Checker {
        word: word_set(grammar),
        boundaries: false,
        empty: Empties::of(grammar, &bodies),
        found: Vec::new(),
    };
    for body in &bodies {
        // Only the literals of rules that make nodes end at a word boundary.
        checker.boundaries = checker.word.is_some() && !body.terminal;
        checker.alternatives(&body.alternatives);
        // A level ends with the step to the next one, as its last alternative.
        if let Some(next) = body.next
            && (body.alternatives.last()).is_some_and(|last| never_fails(last))
        {
            let message = format!(
                "the level {} is never reached from {}, whose last alternative cannot fail",
                next.text, body.name
            );
            checker.warn(DiagnosticCode::UnreachableAlternative, message, next.span);
        }
        for alternative in &body.alternatives {
            checker.repetitions(alternative);
        }
    }
    checker.boundaries = false;
    // The skip expression is the ordered choice of the `%ignore` expressions.
    checker.alternatives(&grammar.ignore.iter().collect::<Vec<_>>());
    for expr in &grammar.ignore {
        checker.repetitions(expr);
    }
    found.append(&mut checker.found);
    found.sort_by_key(|diagnostic| (diagnostic.span.start, diagnostic.span.end));
    found.dedup();
    found
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
/// columns, captures or parameters may fail there, and is not among them.
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
        };
        for (body, number) in bodies.iter().zip(0..) {
            let closed = closing(&body.alternatives);
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
            ExprKind::Choice(_) => (1, closing(&inner).map_or(parts.len(), |last| last + 1)),
            ExprKind::Repeat { min: 0, .. }
            | ExprKind::Separated {
                at_least_one: false,
                ..
            } => (0, 0),
            // One item is a match, whatever comes of the separator after it.
            ExprKind::Separated { .. }
            | ExprKind::Repeat { .. }
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
        (K::Label { label: x, .. }, K::Label { label: y, .. }) => x == y,
        (K::Error { message: x }, K::Error { message: y }) => x == y,
        (x, y) => std::mem::discriminant(x) == std::mem::discriminant(y),
    };
    let (inner_a, inner_b) = (a.children(), b.children());
    same && inner_a.len() == inner_b.len() && inner_a.iter().zip(&inner_b).all(|(x, y)| alike(x, y))
}

/// Whether the expression matches wherever it is tried, or ends the parse with an error, so
/// that no alternative after it is ever tried. Rules are not looked into.
fn never_fails(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Empty => true,
        ExprKind::Sequence(items) => items.iter().all(never_fails),
        ExprKind::Choice(items) => items.iter().any(never_fails),
        ExprKind::Repeat { expr, min, .. } => *min == 0 || never_fails(expr),
        ExprKind::Separated { at_least_one, .. } => !at_least_one,
        // A labeled expression that does not match is an error, not a failure.
        ExprKind::Label { .. } => true,
        ExprKind::Field { expr, .. }
        | ExprKind::FlagOn { expr, .. }
        | ExprKind::FlagOff { expr, .. }
        | ExprKind::Def { expr, .. } => never_fails(expr),
        _ => false,
    }
}

/// The place of the first alternative that cannot fail, which closes the choice: none after
/// it is ever tried.
fn closing(alternatives: &[&Expr]) -> Option<usize> {
    (alternatives.iter()).position(|alternative| never_fails(alternative))
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
        if !starts_with || (rest.is_empty() && first_folds == second_folds) {
            return None;
        }
        // At a word boundary the shorter literal fails where the longer one goes on with a
        // word character.
        if self.boundaries
            && let (Some(word), Some(last), Some(next)) =
                (&self.word, first.chars().last(), rest.chars().next())
            && word.contains(last)
            && word.contains(next)
        {
            return None;
        }
        Some((first.clone(), second.clone()))
    }

    /// The checks of an ordered choice between `alternatives`.
    fn alternatives(&mut self, alternatives: &[&Expr]) {
        for (index, later) in alternatives.iter().enumerate().skip(1) {
            let before = alternatives.get(..index).unwrap_or_default();
            if before.last().is_some_and(|earlier| never_fails(earlier)) {
                let message = "this alternative is never tried, since the one before it cannot \
                               fail";
                let code = DiagnosticCode::UnreachableAlternative;
                self.warn(code, message.to_owned(), later.span);
            }
            let compared = before.len().saturating_sub(MAX_COMPARED_ALTERNATIVES);
            let whole = parts(later);
            for earlier in before.iter().skip(compared) {
                if let Some((first, second)) = self.shadowed(earlier, later) {
                    let message = format!(
                        "{second:?} never matches here, since {first:?} before it matches first"
                    );
                    self.warn(DiagnosticCode::ShadowedLiteral, message, later.span);
                    break;
                }
                let start = parts(earlier);
                if start.len() <= whole.len() && start.iter().zip(&whole).all(|(a, b)| alike(a, b))
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
            self.inside(alternative);
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

    /// The checks of the choices inside an expression.
    fn inside(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Choice(items) => self.alternatives(&items.iter().collect::<Vec<_>>()),
            _ => {
                for child in expr.children() {
                    self.inside(child);
                }
            }
        }
    }
}
