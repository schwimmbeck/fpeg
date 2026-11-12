// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! Runs the case files in `tests/semantics/` on the reference interpreter.
//!
//! A case starts with `=== title`, followed by a grammar. Each `--- "input"` line parses the
//! input, given as a string with Rust escapes, from the rule `start`. The lines after it hold
//! the expected tree or error. `--- spans "input"` shows byte spans in the tree, and
//! `--- grammar` expects the grammar to be rejected with errors that contain the lines after
//! it. Every grammar must also print to text that reads back to the same text.
//!
//! `FASTPEG_BLESS=1 cargo test` rewrites the expectations with the actual results, to review
//! as a diff.

use std::fmt::Write;
use std::path::Path;

use fastpeg::grammar::Grammar;
use fastpeg::reference::Reference;

struct Check {
    line: usize,
    kind: CheckKind,
    expected: Vec<String>,
}

enum CheckKind {
    Parse { input: String, spans: bool },
    GrammarError,
}

struct Case {
    title: String,
    line: usize,
    grammar: String,
    checks: Vec<Check>,
}

fn parse_cases(text: &str) -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        if let Some(title) = line.strip_prefix("=== ") {
            cases.push(Case {
                title: title.to_owned(),
                line: number,
                grammar: String::new(),
                checks: Vec::new(),
            });
            continue;
        }
        let Some(case) = cases.last_mut() else {
            continue;
        };
        if let Some(rest) = line.strip_prefix("---") {
            let rest = rest.trim();
            let kind = if rest == "grammar" {
                CheckKind::GrammarError
            } else {
                let (spans, quoted) = match rest.strip_prefix("spans ") {
                    Some(quoted) => (true, quoted),
                    None => (false, rest),
                };
                CheckKind::Parse {
                    input: unquote(quoted, number),
                    spans,
                }
            };
            case.checks.push(Check {
                line: number,
                kind,
                expected: Vec::new(),
            });
        } else if let Some(check) = case.checks.last_mut() {
            if !line.trim().is_empty() {
                check.expected.push(line.to_owned());
            }
        } else {
            case.grammar.push_str(line);
            case.grammar.push('\n');
        }
    }
    cases
}

/// Decode a string written with Rust escapes, such as `"a\nb"`.
fn unquote(quoted: &str, line: usize) -> String {
    let inner = quoted
        .strip_prefix('"')
        .and_then(|q| q.strip_suffix('"'))
        .unwrap_or_else(|| panic!("line {line}: the input must be a quoted string"));
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some('u') => {
                let hex: String = chars.by_ref().skip(1).take_while(|&c| c != '}').collect();
                let code = u32::from_str_radix(&hex, 16).expect("valid hex in \\u{...}");
                out.push(char::from_u32(code).expect("a valid code point"));
            }
            Some(other) => out.push(other),
            None => panic!("line {line}: the input ends in a backslash"),
        }
    }
    out
}

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
        (CheckKind::Parse { input, spans }, Ok(grammar)) => match Reference::new(grammar) {
            Err(errors) => errors.iter().map(ToString::to_string).collect(),
            Ok(reference) => match reference.parse(input, "start") {
                Ok(tree) => vec![tree.to_text(input, *spans)],
                Err(error) => vec![error.to_string()],
            },
        },
    }
    .into_iter()
    .map(|line| line.replace('\n', "\\n"))
    .collect()
}

fn load(text: &str) -> Result<Grammar, Vec<String>> {
    let grammar = Grammar::from_text(text)
        .map_err(|errors| errors.iter().map(ToString::to_string).collect::<Vec<_>>())?;
    Reference::new(&grammar)
        .map_err(|errors| errors.iter().map(ToString::to_string).collect::<Vec<_>>())?;
    Ok(grammar)
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

/// The grammar's debug form without source spans, so that two readings of different texts
/// compare equal when they mean the same grammar.
fn shape(grammar: &Grammar) -> String {
    let debug = format!("{grammar:?}");
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug.as_str();
    while let Some(at) = rest.find("Span { start: ") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let end = after.find(" }").map_or(after.len(), |e| e + 2);
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// Printing must give text that reads back to the same grammar.
fn check_printing(grammar: &Grammar) -> Option<String> {
    let printed = grammar.to_string();
    match Grammar::from_text(&printed) {
        Ok(again) if shape(&again) == shape(grammar) => None,
        Ok(_) => Some(format!(
            "the printed grammar reads back differently:\n{printed}"
        )),
        Err(errors) => Some(format!(
            "the printed grammar does not read back: {errors:?}\n{printed}"
        )),
    }
}

fn run_file(path: &Path, bless: bool) -> Vec<String> {
    let text = std::fs::read_to_string(path).expect("the case file is readable");
    let cases = parse_cases(&text);
    let mut failures = Vec::new();
    let mut blessed = text.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut replacements: Vec<(usize, usize, Vec<String>)> = Vec::new();
    for case in &cases {
        let grammar = load(&case.grammar);
        if let Ok(g) = &grammar
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
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("semantics");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .expect("the case directory exists")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "txt"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no case files in {}", dir.display());
    let bless = std::env::var_os("FASTPEG_BLESS").is_some();
    let failures: Vec<String> = paths.iter().flat_map(|p| run_file(p, bless)).collect();
    assert!(
        failures.is_empty(),
        "{} failing checks:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
