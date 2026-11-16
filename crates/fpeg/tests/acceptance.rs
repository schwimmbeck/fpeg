// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Every acceptance grammar in `grammars/` reads from its text, prints back to the same grammar
//! and parses its samples. A grammar without a rule `start` that extends no other is a module for
//! others to extend or import from, which resolves but has no samples.
//!
//! The samples of `grammars/<name>.fpeg` are the files in `grammars/samples/<name>/`. Each must
//! parse from the rule `start`, also with CR LF line breaks, and where a file `<sample>.tree`
//! sits beside it, the tree must equal its content. The files in `grammars/samples/<name>/invalid/` must fail to parse.
//! `FPEG_BLESS=1` writes the `.tree` files of samples that have one.
//!
//! The errors of these grammars name their terminals by display names, not by rule names.

mod common;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use common::check_printing;
use fpeg::{Expr, ExprKind, Grammar, Reference, RuleBody, RuleKind};

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.is_file() && path.extension().is_none_or(|e| e != "tree"))
        .collect();
    files.sort();
    files
}

#[test]
fn acceptance_grammars_parse_their_samples() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("grammars");
    let bless = std::env::var_os("FPEG_BLESS").is_some();
    let mut grammars: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("the grammars directory exists")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "fpeg"))
        .collect();
    grammars.sort();
    let mut failures = Vec::new();
    let mut parsed = 0;
    for path in &grammars {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let text = std::fs::read_to_string(path).expect("the grammar is readable");
        let grammar = match Grammar::from_text(&text) {
            Ok(grammar) => grammar,
            Err(errors) => {
                failures.push(format!("{name}: {errors:?}"));
                continue;
            }
        };
        if let Some(problem) = check_printing(&grammar) {
            failures.push(format!("{name}: {problem}"));
        }
        let reference = match Reference::new(&grammar) {
            Ok(reference) => reference,
            Err(errors) => {
                failures.push(format!("{name}: {errors:?}"));
                continue;
            }
        };
        let start = grammar.rules.iter().any(|rule| rule.name.text == "start");
        if !start && grammar.extends.is_none() {
            // A grammar that lost its start by mistake would skip its samples.
            if root.join("samples").join(name).exists() {
                failures.push(format!("{name}: has samples but no rule start"));
            }
            continue;
        }
        let samples = root.join("samples").join(name);
        let mut valid = files_in(&samples);
        // The grammar of grammars reads itself.
        if name == "fpeg" {
            valid.push(path.clone());
        }
        if valid.is_empty() {
            failures.push(format!("{name}: no samples in {}", samples.display()));
        }
        for sample in valid {
            let input = std::fs::read_to_string(&sample).expect("the sample is readable");
            let shown = sample.display();
            let crlf = input.replace('\n', "\r\n");
            if reference.parse(&crlf, "start").is_err() {
                failures.push(format!("{shown}: fails with CR LF line breaks"));
            }
            match reference.parse(&input, "start") {
                Err(error) => failures.push(format!("{shown}: {error}")),
                Ok(tree) => {
                    parsed += 1;
                    let tree_file = sample.with_extension(format!(
                        "{}.tree",
                        sample.extension().and_then(|e| e.to_str()).unwrap_or("")
                    ));
                    if tree_file.exists() {
                        let actual = tree.to_text(&input, false) + "\n";
                        let expected = std::fs::read_to_string(&tree_file).unwrap_or_default();
                        if bless {
                            std::fs::write(&tree_file, &actual).expect("the tree file is writable");
                        } else if actual != expected.replace("\r\n", "\n") {
                            failures.push(format!("{shown}: tree differs:\n{actual}"));
                        }
                    }
                }
            }
        }
        for sample in files_in(&samples.join("invalid")) {
            let input = std::fs::read_to_string(&sample).expect("the sample is readable");
            if reference.parse(&input, "start").is_ok() {
                failures.push(format!("{}: parsed but must fail", sample.display()));
            }
        }
    }
    assert!(parsed > 0, "no samples parsed");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn line_comments_end_at_a_carriage_return() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("grammars");
    let read = |name: &str| {
        std::fs::read_to_string(root.join(format!("{name}.fpeg"))).expect("the grammar is readable")
    };
    let common = "%import common (CPP_COMMENT, SH_COMMENT, SQL_COMMENT, INT, WS)\nstart: INT+\n\
                  %ignore WS | CPP_COMMENT | SH_COMMENT | SQL_COMMENT\n";
    let cases = [
        (read("expressions"), "a // c\n+ b"),
        (read("tiny"), "let a = 1; // c\nlet b = 2;"),
        (read("jsonc"), "[1 // c\n, 2]"),
        (common.to_owned(), "1 // a\n2 # b\n3 -- c\n4"),
    ];
    for (text, input) in &cases {
        let grammar = Grammar::from_text(text).expect("reads");
        let reference = Reference::new(&grammar).expect("resolves");
        let tree = |input: &str| {
            (reference.parse(input, "start"))
                .map(|tree| tree.to_text(input, false))
                .map_err(|error| error.to_string())
        };
        assert_eq!(tree(&input.replace('\n', "\r")), tree(input), "{input:?}");
    }
}

/// The names that `expr` refers to outside a lookahead, where a failure is quiet.
fn referenced<'g>(expr: &'g Expr, names: &mut HashSet<&'g str>) {
    match &expr.kind {
        ExprKind::And(_) | ExprKind::Not(_) => return,
        ExprKind::Reference { name, .. } => {
            names.insert(name.text.as_str());
        }
        _ => {}
    }
    for child in expr.children() {
        referenced(child, names);
    }
}

#[test]
fn errors_name_terminals_by_their_display_names() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("grammars");
    let mut missing = Vec::new();
    for path in files_in(&root) {
        if path.extension().is_none_or(|e| e != "fpeg") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("the grammar is readable");
        let grammar = Grammar::from_text(&text).expect("the grammar reads");
        // Any grammar may import any terminal of a module.
        let module =
            grammar.extends.is_none() && !grammar.rules.iter().any(|r| r.name.text == "start");
        let mut names = HashSet::new();
        for rule in grammar
            .rules
            .iter()
            .filter(|r| r.kind() != RuleKind::Terminal)
        {
            let alternatives: Vec<_> = match &rule.body {
                RuleBody::Alternatives(alternatives) => alternatives.iter().collect(),
                RuleBody::Precedence(levels) => {
                    levels.iter().flat_map(|l| &l.alternatives).collect()
                }
            };
            for alternative in alternatives {
                referenced(&alternative.expr, &mut names);
            }
        }
        // A recovery runs as a syntactic rule, and its errors explain what failed in it.
        for recover in &grammar.recovers {
            referenced(&recover.expr, &mut names);
        }
        for rule in &grammar.rules {
            let listed = module || names.contains(rule.name.text.as_str());
            if rule.kind() == RuleKind::Terminal && rule.display.is_none() && listed {
                missing.push(format!("{}: {}", path.display(), rule.name.text));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "no display name:\n{}",
        missing.join("\n")
    );
}
