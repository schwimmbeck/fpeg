// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Every diagnostic code has its page in `docs/reference/diagnostics/`, and every page there
//! belongs to a code. A page shows a grammar with the problem, a text block with what
//! `fpeg check` prints for it, and the grammar fixed. The first grammar must report the
//! page's code and the text block must hold its messages exactly, while the fix must give
//! neither an error nor a warning.
//! A page without grammars is one no input can trigger. The page of a code that only the Python
//! binding reports shows Python instead, which the docs build runs.
//!
//! `FPEG_BLESS=1 cargo test` rewrites the text blocks with the actual messages, to review as
//! a diff.

use std::path::Path;

use fpeg::{Diagnostic, DiagnosticCode, Grammar, LineIndex};

/// A fenced block of a page: its info string, its body, and the lines it spans.
struct Block {
    info: String,
    body: String,
    lines: std::ops::Range<usize>,
}

fn blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut open: Option<(String, usize, String)> = None;
    for (index, line) in text.lines().enumerate() {
        match &mut open {
            None => {
                if let Some(info) = line.strip_prefix("```") {
                    open = Some((info.to_owned(), index, String::new()));
                }
            }
            Some((info, start, body)) => {
                if line == "```" {
                    blocks.push(Block {
                        info: std::mem::take(info),
                        body: std::mem::take(body),
                        lines: *start..index + 1,
                    });
                    open = None;
                } else {
                    body.push_str(line);
                    body.push('\n');
                }
            }
        }
    }
    blocks
}

/// The problems of a grammar as `fpeg check` prints them for a file `grammar.fpeg`.
fn problems(text: &str) -> Vec<Diagnostic> {
    match Grammar::from_text(text) {
        Err(errors) => errors,
        Ok(grammar) => fpeg::check(&grammar),
    }
}

fn rendered(text: &str, errors: &[Diagnostic]) -> String {
    let lines = LineIndex::new(text);
    let mut out = String::new();
    for error in errors {
        let (line, column) = lines.line_column(text, error.span.start as usize);
        let (severity, message) = (error.severity(), &error.message);
        out.push_str(&format!(
            "grammar.fpeg:{line}:{column}: {severity}[{}]: {message}\n",
            error.code
        ));
    }
    out
}

#[test]
fn every_code_has_a_page_whose_example_and_fix_hold() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/reference/diagnostics");
    let bless = std::env::var_os("FPEG_BLESS").is_some();
    let mut failures = Vec::new();
    for code in DiagnosticCode::ALL {
        let path = dir.join(format!("{code}.md"));
        let Ok(page) = std::fs::read_to_string(&path) else {
            failures.push(format!("{code} has no page {}", path.display()));
            continue;
        };
        let page = page.replace("\r\n", "\n");
        let found = blocks(&page);
        let shown = path.display();
        match found.as_slice() {
            [] if code == DiagnosticCode::Internal => continue,
            // Only the Python binding is given functions, so its page shows Python, which the
            // docs build runs.
            blocks if code == DiagnosticCode::BuilderFunction => {
                if !blocks.iter().any(|block| block.info == "python") {
                    failures.push(format!("{shown}: needs a python block"));
                }
            }
            [problem, output, fix]
                if problem.info == "fpeg" && output.info == "text" && fix.info == "fpeg" =>
            {
                let errors = problems(&problem.body);
                if !errors.iter().any(|e| e.code == code) {
                    failures.push(format!(
                        "{shown}: the grammar does not report {code}: {errors:?}"
                    ));
                }
                let actual = rendered(&problem.body, &errors);
                if actual != output.body {
                    if bless {
                        let mut lines: Vec<&str> = page.lines().collect();
                        let body: Vec<&str> = actual.lines().collect();
                        lines.splice(output.lines.start + 1..output.lines.end - 1, body);
                        let mut text = lines.join("\n");
                        text.push('\n');
                        std::fs::write(&path, text).expect("the page is writable");
                    } else {
                        failures.push(format!("{shown}: check prints\n{actual}"));
                    }
                }
                let fixed = problems(&fix.body);
                if !fixed.is_empty() {
                    failures.push(format!("{shown}: the fix still has problems: {fixed:?}"));
                }
            }
            _ => failures.push(format!(
                "{shown}: needs a fpeg block, a text block and a fpeg block"
            )),
        }
    }
    for entry in std::fs::read_dir(&dir).expect("the diagnostics directory exists") {
        let path = entry.expect("a directory entry").path();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let known = DiagnosticCode::ALL.iter().any(|c| c.code() == stem);
        if stem != "index" && !known {
            failures.push(format!("{} belongs to no code", path.display()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
