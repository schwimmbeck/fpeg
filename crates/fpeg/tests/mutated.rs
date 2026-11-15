// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Grammar texts with one random edit never panic the text front end or the resolver, every
//! edited text that still reads prints back to the same grammar, and the grammars that still
//! resolve parse their samples or fail, without panicking either.
//!
//! The edits start from the grammars in `grammars/`. `FPEG_MUTANTS` sets how many edits each
//! grammar gets and `FPEG_MUTANT_SEED` the first seed.

mod common;

use std::panic::catch_unwind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::check_printing;
use fpeg::{Grammar, Limits, Reference};

/// Edits each grammar gets in a normal test run.
const DEFAULT_MUTANTS: u64 = 12;

/// Steps a parse of a sample may take, so that broken grammars end quickly.
const MAX_STEPS: u64 = 100_000;

/// The largest sample parsed with each edited grammar, so that a normal run stays fast.
const MAX_SAMPLE_BYTES: usize = 1_000;

/// What an edit may insert: the punctuation of the grammar language, letters and layout.
const INSERTS: [&str; 24] = [
    "(", ")", "[", "]", "{", "}", "|", "*", "+", "?", "!", "&", "~", "^", "\"", "\\", "%", "@",
    "<", ">", "=", " ", "\n", "a",
];

/// SplitMix64, as in the generated tests.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// The text with one edit: a character deleted or inserted, a line deleted or doubled, or two
/// neighbouring characters swapped.
fn mutate(text: &str, rng: &mut Rng) -> String {
    let chars: Vec<char> = text.chars().collect();
    let lines: Vec<&str> = text.lines().collect();
    let at = rng.below(chars.len());
    match rng.below(5) {
        0 => chars[..at]
            .iter()
            .chain(&chars[(at + 1).min(chars.len())..])
            .collect(),
        1 => {
            let insert = INSERTS[rng.below(INSERTS.len())];
            let (before, after): (String, String) =
                (chars[..at].iter().collect(), chars[at..].iter().collect());
            before + insert + &after
        }
        2 => {
            let line = rng.below(lines.len());
            let kept = lines.iter().enumerate().filter(|&(i, _)| i != line);
            kept.map(|(_, l)| *l).collect::<Vec<_>>().join("\n")
        }
        3 => {
            let line = rng.below(lines.len());
            let mut doubled = lines.clone();
            doubled.insert(line, lines[line]);
            doubled.join("\n")
        }
        _ => {
            let mut swapped = chars.clone();
            if at + 1 < swapped.len() {
                swapped.swap(at, at + 1);
            }
            swapped.into_iter().collect()
        }
    }
}

fn files(dir: &Path, extension: Option<&str>) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|p| p.is_file())
        .filter(|p| extension.is_none_or(|e| p.extension().is_some_and(|x| x == e)))
        .collect();
    files.sort();
    files
}

fn env_number(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
fn grammar_texts_with_one_edit_never_panic() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("grammars");
    let mutants = env_number("FPEG_MUTANTS", DEFAULT_MUTANTS);
    let first = env_number("FPEG_MUTANT_SEED", 0);
    let limits = Limits {
        max_steps: MAX_STEPS,
        ..Limits::default()
    };
    // Each grammar with its name, its text and the samples it parses.
    let grammars: Vec<(String, String, Vec<String>)> = files(&root, Some("fpeg"))
        .iter()
        .map(|path| {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            // A `.tree` file beside a sample holds its expected tree, no input of the grammar.
            let samples = files(&root.join("samples").join(name), None)
                .iter()
                .filter(|sample| sample.extension().is_none_or(|e| e != "tree"))
                .filter_map(|sample| std::fs::read_to_string(sample).ok())
                .filter(|sample| sample.len() <= MAX_SAMPLE_BYTES)
                .collect();
            let text = std::fs::read_to_string(path).expect("the grammar is readable");
            (name.to_owned(), text, samples)
        })
        .collect();
    let jobs: Vec<(usize, u64)> = (0..grammars.len())
        .flat_map(|g| (first..first + mutants).map(move |seed| (g, seed)))
        .collect();
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                while let Some(&(g, seed)) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let (name, text, samples) = &grammars[g];
                    let mutant = mutate(text, &mut Rng(seed));
                    let outcome = catch_unwind(|| {
                        let grammar = Grammar::from_text(&mutant).ok()?;
                        if let Some(problem) = check_printing(&grammar) {
                            return Some(problem);
                        }
                        if let Ok(reference) = Reference::new(&grammar) {
                            for sample in samples {
                                let _ = reference.parse_with(sample, "start", limits);
                            }
                        }
                        None
                    });
                    let problem = match outcome {
                        Ok(problem) => problem,
                        Err(_) => Some("panicked".to_owned()),
                    };
                    if let Some(problem) = problem {
                        let mut failures = failures.lock().expect("no thread panics holding it");
                        failures.push(format!("{name}, seed {seed}: {problem}\n{mutant}"));
                    }
                }
            });
        }
    });
    let failures = failures
        .into_inner()
        .expect("no thread panicked holding it");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
