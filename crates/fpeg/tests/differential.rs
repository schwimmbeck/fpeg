// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The parsing machine against the reference interpreter: every case file, every acceptance
//! grammar with its samples, and generated grammars with generated inputs give the same tree, or
//! the same error with the same expected set, on both engines, whatever the machine
//! remembers and whichever passes of the optimizer run.
//!
//! Each test reports how many parses it compared and holds the count above a floor, so that a
//! shrinking count shows. The case files and the
//! acceptance grammars stay far below the limits. Generated grammars can reach them, and since
//! the engines count steps differently and only the reference interpreter remembers results,
//! those parses are skipped too, but a parse where only the machine hits a limit is counted
//! on its own and held rare, since a machine that loops shows exactly so.

mod common;

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use common::cases::{CheckKind, case_files, parse_cases};
use common::generator::{check_seeds, generate, reference_limits, sampled_passes};
use fpeg::{
    Grammar, Limit, Limits, Memo, Options, ParseError, ParseErrorKind, Parser, Passes, Reference,
    RuleKind, Tree,
};

/// Steps a parse of a generated grammar may take on the machine. Each instruction is a step,
/// and an instruction does less than a step of the interpreter.
const MACHINE_MAX_STEPS: u64 = 200_000;

/// What the machine remembers in each of the parsers a grammar is compared with.
const MEMO: [Memo; 3] = [Memo::Grammar, Memo::Off, Memo::All];

/// The settings of the parsers a grammar is compared with: each of [`MEMO`], the first with
/// every pass as users get it, the others with passes picked by `sample`, so that every
/// combination meets its share of the grammars.
fn settings(sample: u64) -> [Options; 3] {
    let passes = [
        Passes::ALL,
        sampled_passes(2 * sample),
        sampled_passes(2 * sample + 1),
    ];
    [0, 1, 2].map(|i| Options {
        memo: MEMO[i],
        passes: passes[i],
        ..Options::default()
    })
}

/// What a grammar gives the comparison.
enum Engines {
    /// Both engines reject it, with the same errors.
    Rejected,
    /// The reference, and the machine once for each of the [`settings`].
    Both(Box<Reference>, Vec<(Options, Parser)>),
}

/// The engines for a grammar, with the machine at the [`settings`] of `sample`. The machine
/// must reject exactly what the reference rejects, with the same errors.
fn engines(grammar: &Grammar, sample: u64) -> Result<Engines, String> {
    match (Reference::new(grammar), Parser::new(grammar)) {
        (Ok(reference), Ok(_)) => {
            let parsers: Result<Vec<_>, _> = (settings(sample).into_iter())
                .map(|options| Parser::with_options(grammar, &options).map(|p| (options, p)))
                .collect();
            match parsers {
                Ok(parsers) => Ok(Engines::Both(Box::new(reference), parsers)),
                Err(errors) => Err(format!("a setting rejects it: {errors:?}")),
            }
        }
        (Err(expected), Err(errors)) if expected == errors => Ok(Engines::Rejected),
        (Ok(_), Err(errors)) => Err(format!("the machine rejects it: {errors:?}")),
        (Err(expected), Err(errors)) => Err(format!(
            "the engines reject it differently:\n{expected:?}\n{errors:?}"
        )),
        (Err(expected), Ok(_)) => Err(format!("the machine accepts it, not {expected:?}")),
    }
}

/// A parse as text: the tree with spans and its recoveries, or the error with its expected set
/// and its recoveries, or the limit hit.
fn shown(result: Result<Tree, ParseError>, input: &str) -> Result<String, Limit> {
    match result {
        Ok(tree) => Ok(format!(
            "{}\n{:?}",
            tree.to_text(input, true),
            tree.recoveries()
        )),
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

/// Parse with the reference and with each parser, in recovering mode where `recovering`, and
/// compare. With `remembering`, the parser that remembers nothing is left out.
fn compare(
    reference: &Reference,
    parsers: &[(Options, Parser)],
    input: &str,
    start: &str,
    limits: [Limits; 2],
    remembering: bool,
    recovering: bool,
) -> Vec<Verdict> {
    let expected = match recovering {
        true => reference.parse_recovering(input, start, limits[0]),
        false => reference.parse_with(input, start, limits[0]),
    };
    let expected = shown(expected, input);
    let verdict = |(options, parser): &(Options, Parser)| {
        if remembering && options.memo == Memo::Off {
            return None;
        }
        let actual = match recovering {
            true => parser.parse_recovering_with(input, start, limits[1]),
            false => parser.parse_with(input, start, limits[1]),
        };
        let actual = shown(actual, input);
        Some(match (&expected, actual) {
            (Ok(expected), Ok(actual)) if *expected == actual => Verdict::Same,
            (Ok(expected), Ok(actual)) => Verdict::Differ(format!(
                "on {input:?} the reference gives\n{expected}\nand the machine with \
                 {options:?}\n{actual}\n{}",
                parser.disassemble()
            )),
            (Ok(_), Err(limit)) => Verdict::MachineLimit(limit),
            (Err(_), _) => Verdict::ReferenceLimit,
        })
    };
    parsers.iter().filter_map(verdict).collect()
}

/// How many parses were compared and skipped.
#[derive(Default)]
struct Tally {
    compared: u64,
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
            "{what}: {} parses compared, {} skipped where only the machine and {} where the \
             reference hit a limit",
            self.compared, self.machine_limits, self.reference_limits
        );
    }
}

#[test]
fn case_files_give_the_same_results_on_both_engines() {
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    let mut sample = 0;
    for path in case_files() {
        let text = std::fs::read_to_string(&path).expect("the case file is readable");
        for case in parse_cases(&text) {
            let at = format!("{}:{}: {}", path.display(), case.line, case.title);
            let Ok(grammar) = Grammar::from_text(&case.grammar) else {
                continue;
            };
            sample += 1;
            let (reference, parsers) = match engines(&grammar, sample) {
                Err(problem) => {
                    failures.push(format!("{at}: {problem}"));
                    continue;
                }
                Ok(Engines::Rejected) => continue,
                Ok(Engines::Both(reference, parsers)) => (reference, parsers),
            };
            for check in &case.checks {
                let CheckKind::Parse { input, recover, .. } = &check.kind else {
                    continue;
                };
                let at = format!("{at}, line {}", check.line);
                let limits = default_limits();
                let verdicts = compare(
                    &reference, &parsers, input, "start", limits, false, *recover,
                );
                for verdict in verdicts {
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
    for (sample, path) in (0..).zip(&paths) {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let text = std::fs::read_to_string(path).expect("the grammar is readable");
        let grammar = Grammar::from_text(&text).expect("the grammar reads");
        let (reference, parsers) = match engines(&grammar, sample) {
            Ok(Engines::Rejected) => panic!("{name} does not resolve"),
            Ok(Engines::Both(reference, parsers)) => (reference, parsers),
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
        run.push(name);
        for file in files {
            let input = std::fs::read_to_string(&file).expect("the sample is readable");
            // Without a memo the levels of a precedence block take time that doubles with each
            // level, so the samples are parsed only by the parsers that remember them.
            let limits = default_limits();
            for verdict in compare(&reference, &parsers, &input, "start", limits, true, false) {
                tally.count(verdict, &mut failures, &file.display().to_string());
            }
        }
    }
    tally.report(&format!("acceptance grammars ({} run)", run.join(", ")));
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(run.len() >= 10, "only {} grammars run", run.len());
}

/// The counts of the generated grammars, which every thread adds to.
static COMPARED: AtomicU64 = AtomicU64::new(0);
static MACHINE_LIMITS: AtomicU64 = AtomicU64::new(0);
static REFERENCE_LIMITS: AtomicU64 = AtomicU64::new(0);

fn check_against_reference(seed: u64) -> Option<String> {
    let (grammar, inputs) = generate(seed);
    let (reference, parsers) = match engines(&grammar, seed) {
        Err(problem) => return Some(format!("{problem}\n{grammar}")),
        Ok(Engines::Rejected) => return None,
        Ok(Engines::Both(reference, parsers)) => (reference, parsers),
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
            for verdict in compare(&reference, &parsers, input, start, limits, false, false) {
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
        machine_limits: MACHINE_LIMITS.load(Ordering::Relaxed),
        reference_limits: REFERENCE_LIMITS.load(Ordering::Relaxed),
    };
    tally.report("generated grammars");
    let all = tally.compared + tally.machine_limits + tally.reference_limits;
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
