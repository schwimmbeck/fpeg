// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Runs the case files in `tests/semantics/` on the reference interpreter. `common/cases.rs`
//! describes their format. Every grammar must also print to text that reads back to the same
//! grammar.
//!
//! `FPEG_BLESS=1 cargo test` rewrites the expectations with the actual results, to review
//! as a diff.

mod common;

use std::fmt::Write;
use std::path::Path;

use common::cases::{Check, CheckKind, case_files, parse_cases};
use common::check_printing;
use fpeg::{Diagnostic, Grammar, Limits, ParseErrorKind, Parser, Recovery, Reference};

/// What a check produces, as the lines the case file holds.
fn actual(grammar: &Result<Grammar, Vec<String>>, check: &Check) -> Vec<String> {
    match (&check.kind, grammar) {
        (CheckKind::GrammarError, Err(errors)) => errors.clone(),
        (CheckKind::GrammarError, Ok(_)) => vec!["the grammar was accepted".to_owned()],
        (CheckKind::Parse { .. }, Err(errors)) => {
            let mut lines = vec!["the grammar was rejected:".to_owned()];
            lines.extend(errors.iter().cloned());
            lines
        }
        (
            CheckKind::Parse {
                input,
                spans,
                recover,
            },
            Ok(grammar),
        ) => match Reference::new(grammar) {
            Err(errors) => errors.iter().map(ToString::to_string).collect(),
            Ok(reference) => {
                let result = if *recover {
                    let limits = Limits {
                        max_steps: Reference::DEFAULT_MAX_STEPS,
                        ..Limits::default()
                    };
                    reference.parse_recovering(input, "start", limits)
                } else {
                    reference.parse(input, "start")
                };
                match result {
                    Ok(tree) => [tree.to_text(input, *spans)]
                        .into_iter()
                        .chain(recovered(tree.recoveries()))
                        .collect(),
                    Err(error) => [error.to_string()]
                        .into_iter()
                        .chain(recovered(&error.recoveries))
                        .collect(),
                }
            }
        },
    }
    .into_iter()
    .map(|line| line.replace('\n', "\\n"))
    .collect()
}

/// The line that lists what a parse recovered from, if it recovered.
fn recovered(recoveries: &[Recovery]) -> Option<String> {
    let each: Vec<String> = (recoveries.iter())
        .map(|r| format!("{} at {}", r.message(), r.position))
        .collect();
    (!each.is_empty()).then(|| format!("errors: {}", each.join(", ")))
}

/// The grammar if it resolves, else the errors, and the grammar as read, if it reads, since a
/// grammar that does not resolve must still print back.
fn load(text: &str) -> (Result<Grammar, Vec<String>>, Option<Grammar>) {
    let strings = |errors: Vec<Diagnostic>| errors.iter().map(ToString::to_string).collect();
    match Grammar::from_text(text) {
        Err(errors) => (Err(strings(errors)), None),
        Ok(grammar) => match Reference::new(&grammar) {
            Ok(_) => (Ok(grammar.clone()), Some(grammar)),
            Err(errors) => (Err(strings(errors)), Some(grammar)),
        },
    }
}

fn matches(expected: &[String], actual: &[String], grammar_error: bool) -> bool {
    if grammar_error {
        // Each expected line must occur in some error message.
        return !expected.is_empty()
            && expected
                .iter()
                .all(|e| actual.iter().any(|a| a.contains(e.trim())));
    }
    expected == actual
}

fn run_file(path: &Path, bless: bool) -> Vec<String> {
    let text = std::fs::read_to_string(path).expect("the case file is readable");
    let cases = parse_cases(&text);
    let mut failures = Vec::new();
    let mut blessed = text.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut replacements: Vec<(usize, usize, Vec<String>)> = Vec::new();
    for case in &cases {
        let (grammar, read) = load(&case.grammar);
        if let Some(g) = &read
            && let Some(problem) = check_printing(g)
        {
            failures.push(format!(
                "{}:{}: {}: {problem}",
                path.display(),
                case.line,
                case.title
            ));
        }
        for check in &case.checks {
            let got = actual(&grammar, check);
            let grammar_error = matches!(check.kind, CheckKind::GrammarError);
            if !matches(&check.expected, &got, grammar_error) {
                let mut message = format!(
                    "{}:{}: {}\n  expected:\n",
                    path.display(),
                    check.line,
                    case.title
                );
                for line in &check.expected {
                    let _ = writeln!(message, "    {line}");
                }
                message.push_str("  actual:\n");
                for line in &got {
                    let _ = writeln!(message, "    {line}");
                }
                failures.push(message);
                replacements.push((check.line, check.expected.len(), got));
            }
        }
    }
    if bless && !replacements.is_empty() {
        // Replace from the bottom so that earlier line numbers stay valid.
        for (line, count, got) in replacements.into_iter().rev() {
            let start = line;
            let mut end = start;
            let mut seen = 0;
            while seen < count && end < blessed.len() {
                if !blessed[end].trim().is_empty() {
                    seen += 1;
                }
                end += 1;
            }
            blessed.splice(start..end, got);
        }
        let mut out = blessed.join("\n");
        out.push('\n');
        std::fs::write(path, out).expect("the case file is writable");
    }
    failures
}

#[test]
fn case_files_match_the_reference_interpreter() {
    let paths = case_files();
    let bless = std::env::var_os("FPEG_BLESS").is_some();
    let failures: Vec<String> = paths.iter().flat_map(|p| run_file(p, bless)).collect();
    assert!(
        failures.is_empty(),
        "{} failing checks:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// The case files always start from `start`, so these cases are written out.
#[test]
fn a_parse_starts_from_a_rule_or_a_level_but_not_from_an_unknown_name() {
    let text = "start: expr\nexpr: %precedence {\n    sum: left=sum \"+\" right=atom -> add\n    atom: N\n}\nN: [0-9]\n";
    let grammar = Grammar::from_text(text).expect("the grammar reads");
    let reference = Reference::new(&grammar).expect("the grammar resolves");
    let tree = reference
        .parse("1+2", "sum")
        .expect("a level starts a parse");
    assert_eq!(
        tree.to_text("1+2", false),
        "(add left=N\"1\" ~\"+\" right=N\"2\")"
    );
    let unknown = ParseErrorKind::UnknownStart("missing".to_owned());
    let error = reference.parse("1", "missing").expect_err("no such rule");
    assert_eq!(error.kind, unknown);
    // The machine starts from the same level, and knows which names exist.
    let parser = Parser::new(&grammar).expect("the grammar compiles");
    let tree = parser.parse("1+2", "sum").expect("a level starts a parse");
    assert_eq!(
        tree.to_text("1+2", false),
        "(add left=N\"1\" ~\"+\" right=N\"2\")"
    );
    let error = parser.parse("1", "missing").expect_err("no such rule");
    assert_eq!(error.message(), "no rule missing to start from");
    assert_eq!(error.kind, unknown);
}

// The case files match grammar errors by subset, so these exact lists are written out.
#[test]
fn a_grammar_text_with_three_syntax_errors_gives_three() {
    let text = "start: \"a\" |\nother: \"b\" )\n%flag\nlast: \"c\"\n";
    let errors = Grammar::from_text(text).expect_err("three mistakes");
    let starts: Vec<u32> = errors.iter().map(|e| e.span.start).collect();
    assert_eq!(starts, [12, 24, 31], "{errors:?}");
}

#[test]
fn a_precedence_block_with_parameters_reports_only_that() {
    let text = "start: t<\"x\">\nt<p>: %precedence {\n    b: p\n}\n";
    let grammar = Grammar::from_text(text).expect("the grammar reads");
    let errors = Reference::new(&grammar).expect_err("parameters are an error");
    let messages: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(messages, ["the precedence block t cannot have parameters"]);
}

#[test]
fn an_inlined_rule_marked_transparent_reports_only_that() {
    let text = "start: _pair\n?_pair: key=\"a\" value=\"b\"\n";
    let grammar = Grammar::from_text(text).expect("the grammar reads");
    let errors = Reference::new(&grammar).expect_err("an inlined rule makes no node to give way");
    let messages: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(messages, ["the inlined rule _pair cannot be transparent"]);
}
