// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Checking a grammar: its errors, and warnings about what works and likely not as meant.
//!
//! A warning says only what holds for every input. Whether an alternative can ever match is
//! undecidable in general, so each check is a named case that can be told from the grammar as
//! written, and what it cannot tell it leaves alone.

use crate::diag::{Diagnostic, DiagnosticCode};
use crate::grammar::{Expr, ExprKind, Grammar, RuleBody, Span};
use crate::vm::{Options, compile};

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
    let mut checker = Checker { found: Vec::new() };
    for rule in &grammar.rules {
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
    // The skip expression is the ordered choice of the `%ignore` expressions.
    checker.alternatives(&grammar.ignore.iter().collect::<Vec<_>>());
    found.append(&mut checker.found);
    found.sort_by_key(|diagnostic| (diagnostic.span.start, diagnostic.span.end));
    found.dedup();
    found
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
