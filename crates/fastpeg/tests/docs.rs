// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Every grammar block in the docs must read, and one with a `start` rule must also resolve,
//! so that an example in the docs cannot drift from the language.

use std::path::{Path, PathBuf};

use fastpeg::grammar::Grammar;
use fastpeg::reference::Reference;

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("the docs directory is readable") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() && path.file_name().is_some_and(|n| n != "_build") {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

/// The grammar blocks of a page with the line each starts on.
fn grammar_blocks(text: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut current: Option<(usize, String)> = None;
    for (index, line) in text.lines().enumerate() {
        match &mut current {
            None if line.trim_start() == "```fpeg" => current = Some((index + 2, String::new())),
            Some((start, body)) if line.trim_start() == "```" => {
                blocks.push((*start, std::mem::take(body)));
                current = None;
            }
            Some((_, body)) => {
                body.push_str(line);
                body.push('\n');
            }
            None => {}
        }
    }
    blocks
}

#[test]
fn grammar_blocks_in_the_docs_read_and_complete_ones_resolve() {
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    let mut files = Vec::new();
    markdown_files(&docs, &mut files);
    let mut checked = 0;
    let mut failures = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).expect("the page is readable");
        for (line, block) in grammar_blocks(&text) {
            checked += 1;
            let at = format!("{}:{line}", file.display());
            match Grammar::from_text(&block) {
                Err(errors) => failures.push(format!("{at}: {errors:?}")),
                Ok(grammar) if grammar.rules.iter().any(|r| r.name.text == "start") => {
                    if let Err(errors) = Reference::new(&grammar) {
                        failures.push(format!("{at}: {errors:?}"));
                    }
                }
                Ok(_) => {}
            }
        }
    }
    assert!(
        checked > 0,
        "no grammar blocks found under {}",
        docs.display()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
