// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Checking a grammar: its errors, and warnings about what works and likely not as meant.
//!
//! A warning says only what holds for every input. Whether an alternative can ever match is
//! undecidable in general, so each check is a named case that can be told from the grammar as
//! written, and what it cannot tell it leaves alone.

use crate::diag::{Diagnostic, DiagnosticCode};
use crate::grammar::{Expr, ExprKind, Grammar, RuleBody, RuleKind, Span};
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
    let mut checker = Checker {
        word: word_set(grammar),
        boundaries: false,
        found: Vec::new(),
    };
    for rule in &grammar.rules {
        // Only the literals of rules that make nodes end at a word boundary.
        checker.boundaries = checker.word.is_some() && rule.kind() != RuleKind::Terminal;
        match &rule.body {
            RuleBody::Alternatives(alternatives) => {
                let exprs: Vec<&Expr> = alternatives.iter().map(|a| &a.expr).collect();
                checker.alternatives(&exprs);
            }
            RuleBody::Precedence(levels) => {
                for (index, level) in levels.iter().enumerate() {
                    let exprs: Vec<&Expr> = level.alternatives.iter().map(|a| &a.expr).collect();
                    checker.alternatives(&exprs);
                    // A level ends with the step to the next one, as its last alternative.
                    if let Some(next) = levels.get(index + 1)
                        && exprs.last().is_some_and(|last| never_fails(last))
                    {
                        let message = format!(
                            "the level {} is never reached from {}, whose last alternative \
                             cannot fail",
                            next.name.text, level.name.text
                        );
                        let code = DiagnosticCode::UnreachableAlternative;
                        checker.warn(code, message, next.name.span);
                    }
                }
            }
        }
    }
    checker.boundaries = false;
    // The skip expression is the ordered choice of the `%ignore` expressions.
    checker.alternatives(&grammar.ignore.iter().collect::<Vec<_>>());
    found.append(&mut checker.found);
    found.sort_by_key(|diagnostic| (diagnostic.span.start, diagnostic.span.end));
    found.dedup();
    found
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

struct Checker {
    /// The word characters, if the grammar sets them.
    word: Option<CharSet>,
    /// Whether a literal that ends in a word character fails before another one.
    boundaries: bool,
    found: Vec<Diagnostic>,
}

impl Checker {
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
