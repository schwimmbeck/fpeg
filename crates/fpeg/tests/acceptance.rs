// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Every acceptance grammar in `grammars/` reads from its text, prints back to the same grammar
//! and parses its samples.
//!
//! The samples of `grammars/<name>.fpeg` are the files in `grammars/samples/<name>/`. Each must
//! parse from the rule `start`, and where a file `<sample>.tree` sits beside it, the tree must
//! equal its content. The files in `grammars/samples/<name>/invalid/` must fail to parse.
//! `FPEG_BLESS=1` writes the `.tree` files of samples that have one.

mod common;

use std::path::{Path, PathBuf};

use common::check_printing;
use fpeg::{Grammar, Reference};

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
        let samples = root.join("samples").join(name);
        let valid = files_in(&samples);
        if valid.is_empty() {
            failures.push(format!("{name}: no samples in {}", samples.display()));
        }
        for sample in valid {
            let input = std::fs::read_to_string(&sample).expect("the sample is readable");
            let shown = sample.display();
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
