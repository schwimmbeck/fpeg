// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The case files in `tests/semantics/`.
//!
//! A case starts with `=== title`, followed by a grammar. Each `--- "input"` line parses the
//! input, given as a string with Rust escapes, from the rule `start`. The lines after it hold
//! the expected tree or error. `--- spans "input"` shows byte spans in the tree, and
//! `--- grammar` expects the grammar to be rejected with errors that contain the lines after
//! it.

use std::path::{Path, PathBuf};

/// One check of a case: the line it starts on, what it does and the lines it expects.
pub struct Check {
    pub line: usize,
    pub kind: CheckKind,
    pub expected: Vec<String>,
}

/// What a check does.
pub enum CheckKind {
    Parse { input: String, spans: bool },
    GrammarError,
}

/// A case: a grammar and its checks.
pub struct Case {
    pub title: String,
    pub line: usize,
    pub grammar: String,
    pub checks: Vec<Check>,
}

/// Read the cases of a case file.
pub fn parse_cases(text: &str) -> Vec<Case> {
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

/// The case files, sorted.
pub fn case_files() -> Vec<PathBuf> {
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
    paths
}
