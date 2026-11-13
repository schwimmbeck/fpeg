// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The parsing machine against the reference interpreter: every case file, every acceptance
//! grammar with its samples, and generated grammars with generated inputs give the same tree, or
//! the same error with the same expected set, on both engines, whatever the machine
//! remembers.
//!
//! Grammars the machine does not run yet are skipped, and each test reports how many and holds
//! what it compares above a floor, so that a growing count shows. The case files and the
//! acceptance grammars stay far below the limits. Generated grammars can reach them, and since
//! the engines count steps differently and only the reference interpreter remembers results,
//! those parses are skipped too, but a parse where only the machine hits a limit is counted
//! on its own and held rare, since a machine that loops shows exactly so.

mod common;

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use common::cases::{CheckKind, case_files, parse_cases};
use common::generator::{Features, check_seeds, generate_with, reference_limits};
use fpeg::{
    Grammar, Limit, Limits, Memo, Options, ParseError, ParseErrorKind, Parser, Reference, RuleKind,
    Tree,
};

/// What the machine reports for what it does not run yet.
const NOT_YET: &str = "which the parser does not run yet";

/// Steps a parse of a generated grammar may take on the machine. Each instruction is a step,
/// and an instruction does less than a step of the interpreter.
const MACHINE_MAX_STEPS: u64 = 200_000;

/// What the machine remembers in each of the parsers a grammar is compared with.
const MEMO: [Memo; 3] = [Memo::Grammar, Memo::Off, Memo::All];

/// What a grammar gives the comparison.
enum Engines {
    /// Both engines reject it, with the same errors.
    Rejected,
    /// The reference runs it and the machine does not yet.
    Waiting,
    /// The reference, and the machine once for each setting of [`MEMO`].
    Both(Box<Reference>, Vec<Parser>),
}

/// The engines for a grammar. The machine must reject what the reference rejects with the same
/// errors, and otherwise only what waits for a milestone.
fn engines(grammar: &Grammar) -> Result<Engines, String> {
    match (Reference::new(grammar), Parser::new(grammar)) {
        (Ok(reference), Ok(_)) => {
            let parsers: Result<Vec<Parser>, _> = (MEMO.iter())
                .map(|&memo| Parser::with_options(grammar, &Options { memo }))
                .collect();
            match parsers {
                Ok(parsers) => Ok(Engines::Both(Box::new(reference), parsers)),
                Err(errors) => Err(format!("a memo setting rejects it: {errors:?}")),
            }
        }
        (Ok(_), Err(errors)) if errors.iter().all(|e| e.message.contains(NOT_YET)) => {
            Ok(Engines::Waiting)
        }
        (Err(expected), Err(errors)) if expected == errors => Ok(Engines::Rejected),
        (Ok(_), Err(errors)) => Err(format!("the machine rejects it: {errors:?}")),
        (Err(expected), Err(errors)) => Err(format!(
            "the engines reject it differently:\n{expected:?}\n{errors:?}"
        )),
        (Err(expected), Ok(_)) => Err(format!("the machine accepts it, not {expected:?}")),
    }
}

/// A parse as text: the tree with spans, or the error with its expected set, or the limit hit.
fn shown(result: Result<Tree, ParseError>, input: &str) -> Result<String, Limit> {
    match result {
        Ok(tree) => Ok(tree.to_text(input, true)),
        Err(ParseError {
            kind: ParseErrorKind::Limit(limit),
            ..
        }) => Err(limit),
        Err(error) => Ok(format!("{error:?}")),
    }
}

enum Verdict {
    Same,
    /// Only the machine hit this limit.
    MachineLimit(Limit),
    /// The reference interpreter hit a limit.
    ReferenceLimit,
    Differ(String),
}

/// The limits for the engines: the reference interpreter's own step budget, and the machine's.
fn default_limits() -> [Limits; 2] {
    let reference = Limits {
        max_steps: Reference::DEFAULT_MAX_STEPS,
        ..Limits::default()
    };
    [reference, Limits::default()]
}

/// Parse with the reference and with each parser, and compare. With `remembering`, the parser
/// that remembers nothing is left out.
fn compare(
    reference: &Reference,
    parsers: &[Parser],
    input: &str,
    start: &str,
    limits: [Limits; 2],
    remembering: bool,
) -> Vec<Verdict> {
    let expected = shown(reference.parse_with(input, start, limits[0]), input);
    let verdict = |(parser, memo): (&Parser, &Memo)| {
        if remembering && *memo == Memo::Off {
            return None;
        }
        let actual = shown(parser.parse_with(input, start, limits[1]), input);
        Some(match (&expected, actual) {
            (Ok(expected), Ok(actual)) if *expected == actual => Verdict::Same,
            (Ok(expected), Ok(actual)) => Verdict::Differ(format!(
                "on {input:?} the reference gives\n{expected}\nand the machine, remembering \
                 {memo:?},\n{actual}\n{}",
                parser.disassemble()
            )),
            (Ok(_), Err(limit)) => Verdict::MachineLimit(limit),
            (Err(_), _) => Verdict::ReferenceLimit,
        })
    };
    parsers.iter().zip(&MEMO).filter_map(verdict).collect()
}

/// How many parses were compared and skipped.
#[derive(Default)]
struct Tally {
    compared: u64,
    waiting: u64,
    machine_limits: u64,
    reference_limits: u64,
}

impl Tally {
    /// Count a parse of a case file or an acceptance grammar, where a limit is a failure.
    fn count(&mut self, verdict: Verdict, failures: &mut Vec<String>, at: &str) {
        match verdict {
            Verdict::Same => self.compared += 1,
            Verdict::MachineLimit(limit) => {
                failures.push(format!("{at}: the machine hits the {limit:?} limit"));
            }
            Verdict::ReferenceLimit => failures.push(format!("{at}: the reference hits a limit")),
            Verdict::Differ(problem) => failures.push(format!("{at}: {problem}")),
        }
    }

    fn report(&self, what: &str) {
        eprintln!(
            "{what}: {} parses compared, {} skipped in grammars the machine does not run yet, \
             {} skipped where only the machine and {} where the reference hit a limit",
            self.compared, self.waiting, self.machine_limits, self.reference_limits
        );
    }
}

#[test]
fn case_files_give_the_same_results_on_both_engines() {
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    for path in case_files() {
        let text = std::fs::read_to_string(&path).expect("the case file is readable");
        for case in parse_cases(&text) {
            let at = format!("{}:{}: {}", path.display(), case.line, case.title);
            let Ok(grammar) = Grammar::from_text(&case.grammar) else {
                continue;
            };
            let engines = match engines(&grammar) {
                Err(problem) => {
                    failures.push(format!("{at}: {problem}"));
                    continue;
                }
                Ok(Engines::Rejected) => continue,
                Ok(engines) => engines,
            };
            for check in &case.checks {
                let CheckKind::Parse { input, .. } = &check.kind else {
                    continue;
                };
                let Engines::Both(reference, parsers) = &engines else {
                    tally.waiting += 1;
                    continue;
                };
                let at = format!("{at}, line {}", check.line);
                let limits = default_limits();
                for verdict in compare(reference, parsers, input, "start", limits, false) {
                    tally.count(verdict, &mut failures, &at);
                }
            }
        }
    }
    tally.report("case files");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(
        tally.compared >= 3 * 95,
        "only {} parses compared",
        tally.compared
    );
}

#[test]
fn acceptance_grammars_give_the_same_results_on_both_engines() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("grammars");
    let mut paths: Vec<_> = std::fs::read_dir(&root)
        .expect("the grammars directory exists")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "fpeg"))
        .collect();
    paths.sort();
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    let mut run = Vec::new();
    for path in &paths {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let text = std::fs::read_to_string(path).expect("the grammar is readable");
        let grammar = Grammar::from_text(&text).expect("the grammar reads");
        let engines = match engines(&grammar) {
            Ok(Engines::Rejected) => panic!("{name} does not resolve"),
            Ok(engines) => engines,
            Err(problem) => {
                failures.push(format!("{name}: {problem}"));
                continue;
            }
        };
        let samples = root.join("samples").join(name);
        let files: Vec<_> = [samples.clone(), samples.join("invalid")]
            .iter()
            .filter_map(|dir| std::fs::read_dir(dir).ok())
            .flatten()
            .map(|entry| entry.expect("a directory entry").path())
            .filter(|path| path.is_file() && path.extension().is_none_or(|e| e != "tree"))
            .collect();
        let Engines::Both(reference, parsers) = engines else {
            tally.waiting += files.len() as u64;
            continue;
        };
        run.push(name);
        for file in files {
            let input = std::fs::read_to_string(&file).expect("the sample is readable");
            // Without a memo the levels of a precedence block take time that doubles with each
            // level, so the samples are parsed only by the parsers that remember them.
            let limits = default_limits();
            for verdict in compare(&reference, &parsers, &input, "start", limits, true) {
                tally.count(verdict, &mut failures, &file.display().to_string());
            }
        }
    }
    tally.report(&format!("acceptance grammars ({} run)", run.join(", ")));
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    for name in ["csv", "expressions", "fpeg", "json", "sql", "tiny", "toml"] {
        assert!(run.contains(&name), "the machine does not run {name}");
    }
}

/// Grammars a seed tries until the machine runs one.
const ATTEMPTS: u64 = 8;

/// The counts of the generated grammars, which every thread adds to.
static COMPARED: AtomicU64 = AtomicU64::new(0);
static WAITING: AtomicU64 = AtomicU64::new(0);
static MACHINE_LIMITS: AtomicU64 = AtomicU64::new(0);
static REFERENCE_LIMITS: AtomicU64 = AtomicU64::new(0);

fn check_against_reference(seed: u64) -> Option<String> {
    // Rules that call each other at random are often left recursive, so a seed tries a few
    // grammars until the machine runs one.
    let mut attempt = 0;
    let (grammar, inputs, reference, parsers) = loop {
        let (grammar, inputs) = generate_with(seed * ATTEMPTS + attempt, Features::MACHINE);
        attempt += 1;
        match engines(&grammar) {
            Err(problem) => return Some(format!("{problem}\n{grammar}")),
            Ok(Engines::Rejected) => return None,
            Ok(Engines::Both(reference, parsers)) => break (grammar, inputs, reference, parsers),
            Ok(Engines::Waiting) if attempt < ATTEMPTS => {}
            Ok(Engines::Waiting) => {
                WAITING.fetch_add(inputs.len() as u64, Ordering::Relaxed);
                return None;
            }
        }
    };
    let machine = Limits {
        max_steps: MACHINE_MAX_STEPS,
        ..Limits::default()
    };
    // A parse from a terminal opens its token on an empty stack, which a start rule never does.
    let terminal = (grammar.rules.iter())
        .find(|r| r.kind() == RuleKind::Terminal && r.params.is_empty())
        .map(|r| r.name.text.as_str());
    for start in std::iter::once("start").chain(terminal) {
        for input in &inputs {
            let limits = [reference_limits(), machine];
            for verdict in compare(&reference, &parsers, input, start, limits, false) {
                match verdict {
                    Verdict::Same => COMPARED.fetch_add(1, Ordering::Relaxed),
                    // Inputs this short reach the depth limit only where left recursion is not
                    // seen as such and descends without end.
                    Verdict::MachineLimit(Limit::Depth) => {
                        return Some(format!(
                            "from {start} on {input:?} only the machine hits the depth limit\n\
                             {grammar}"
                        ));
                    }
                    Verdict::MachineLimit(_) => MACHINE_LIMITS.fetch_add(1, Ordering::Relaxed),
                    Verdict::ReferenceLimit => REFERENCE_LIMITS.fetch_add(1, Ordering::Relaxed),
                    Verdict::Differ(problem) => {
                        return Some(format!("from {start}: {problem}\n{grammar}"));
                    }
                };
            }
        }
    }
    None
}

#[test]
fn generated_grammars_give_the_same_results_on_both_engines() {
    check_seeds(check_against_reference);
    let tally = Tally {
        compared: COMPARED.load(Ordering::Relaxed),
        waiting: WAITING.load(Ordering::Relaxed),
        machine_limits: MACHINE_LIMITS.load(Ordering::Relaxed),
        reference_limits: REFERENCE_LIMITS.load(Ordering::Relaxed),
    };
    tally.report("generated grammars");
    let all = tally.compared + tally.waiting + tally.machine_limits + tally.reference_limits;
    assert!(
        tally.compared * 2 >= all,
        "only {} of {all} parses compared",
        tally.compared
    );
    // Without a memo the machine backtracks more, which a few grammars need beyond its limit.
    assert!(
        tally.machine_limits * 200 <= tally.compared,
        "the machine alone hits a limit in {} parses",
        tally.machine_limits
    );
}
