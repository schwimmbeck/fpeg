// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The passes of fpeg's optimizer in Rust: each workload parsed with every pass, and with each
//! pass left out, so that every pass shows what it brings. JSON, the expressions and Python
//! take the inputs of the other suites, the rest the samples of their grammars, repeated to
//! about 100 KB. A grammar that repeats work its memo does not save shows the restart.

use std::path::Path;
use std::time::Duration;

use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};
use fpeg::{Options, Passes};
use fpeg_benchmarks::{EXPRESSIONS_PATH, PYTHON_FILE_PATH, RECORDS_JSON_PATH};

/// Samples per setting of a workload.
const SAMPLES: usize = 10;

/// Time criterion measures each setting of a workload for.
const MEASUREMENT_TIME_S: u64 = 3;

/// Time criterion warms each setting of a workload up for.
const WARM_UP_TIME_S: u64 = 1;

/// The size the samples of a grammar are repeated to.
const REPEATED_BYTES: usize = 100_000;

/// Levels of nesting of the grammar that repeats work, each of which doubles it without the
/// restart.
const RESTART_LEVELS: usize = 24;

/// What turning one pass off does to the passes.
type TurnOff = fn(&mut Passes);

/// Each pass, with what turning it off does to the passes.
const PASSES: [(&str, TurnOff); 7] = [
    ("hasher", |p| p.hasher = false),
    ("span", |p| p.span = false),
    ("skip", |p| p.skip = false),
    ("dispatch", |p| p.dispatch = false),
    ("inline", |p| p.inline = false),
    ("split", |p| p.split = false),
    ("left_loop", |p| p.left_loop = false),
];

/// The folder of the acceptance grammars and their samples.
fn grammars() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/fpeg/grammars"
    ))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The files of a folder whose names end in `extension`, read in the order of their names.
fn files(folder: &Path, extension: &str) -> Vec<String> {
    let entries = std::fs::read_dir(folder).expect("the folder exists");
    let mut paths: Vec<_> = (entries.map(|entry| entry.expect("an entry").path()))
        .filter(|path| path.extension().is_some_and(|e| e == extension))
        .collect();
    paths.sort();
    paths.iter().map(|path| read(path)).collect()
}

/// The texts joined by new lines, as often as it takes to reach [`REPEATED_BYTES`].
fn repeated(texts: &[String]) -> String {
    let once = texts.join("\n");
    let mut text = once.clone();
    while text.len() < REPEATED_BYTES {
        text += "\n";
        text += &once;
    }
    text
}

/// The workloads: a name, a grammar and an input.
fn workloads() -> Vec<(&'static str, String, String)> {
    let samples = grammars().join("samples");
    let grammar = |name: &str| read(&grammars().join(format!("{name}.fpeg")));
    // A query that ends a file may lack the semicolon that a query before another needs.
    let sql = ["queries.sql", "small.sql"].map(|name| {
        let text = read(&samples.join("sql").join(name));
        format!("{};", text.trim_end().trim_end_matches(';'))
    });
    vec![
        ("JSON", grammar("json"), read(Path::new(RECORDS_JSON_PATH))),
        (
            "expressions",
            grammar("expressions"),
            read(Path::new(EXPRESSIONS_PATH)),
        ),
        ("SQL", grammar("sql"), repeated(&sql)),
        (
            "tiny",
            grammar("tiny"),
            repeated(&files(&samples.join("tiny"), "tiny")),
        ),
        (
            "TOML",
            grammar("toml"),
            repeated(&[read(&samples.join("toml").join("config.toml"))]),
        ),
        (
            "the grammar files",
            grammar("fpeg"),
            repeated(&files(grammars(), "fpeg")),
        ),
        (
            "CSV",
            grammar("csv"),
            repeated(&files(&samples.join("csv"), "csv")),
        ),
        (
            "Python",
            grammar("python"),
            read(Path::new(PYTHON_FILE_PATH)),
        ),
        // The program alone, since the other sample ends in a return, which ends a chunk.
        (
            "Lua",
            grammar("lua"),
            repeated(&[read(&samples.join("lua").join("program.lua"))]),
        ),
        (
            "the outline language",
            grammar("outline"),
            repeated(&files(&samples.join("outline"), "outline")),
        ),
    ]
}

fn parser(grammar: &str, options: &Options) -> fpeg::Parser {
    let grammar = fpeg::Grammar::from_text(grammar).expect("the grammar reads");
    fpeg::Parser::with_options(&grammar, options).expect("the grammar compiles")
}

fn passes(c: &mut Criterion) {
    for (name, grammar, input) in workloads() {
        let mut group = c.benchmark_group(format!("passes on {name}"));
        group
            .sample_size(SAMPLES)
            .sampling_mode(SamplingMode::Flat)
            .warm_up_time(Duration::from_secs(WARM_UP_TIME_S))
            .measurement_time(Duration::from_secs(MEASUREMENT_TIME_S))
            .throughput(Throughput::Bytes(input.len() as u64));
        let mut settings = vec![("every pass".to_owned(), Passes::ALL)];
        for (pass, turn_off) in PASSES {
            let mut passes = Passes::ALL;
            turn_off(&mut passes);
            settings.push((format!("without {pass}"), passes));
        }
        for (setting, passes) in settings {
            let options = Options {
                passes,
                ..Options::default()
            };
            let parser = parser(&grammar, &options);
            group.bench_function(setting, |b| {
                b.iter(|| {
                    parser
                        .parse(&input, "start")
                        .expect("the input parses")
                        .len()
                })
            });
        }
        group.finish();
    }
    // `b` is matched twice at every level, which nothing remembered by default saves, until
    // the parse runs out of its budget and starts again with every rule remembered.
    let grammar = "start: a\na: c | d\nc: b \"x\"\nd: b \"y\"\nb: \"(\" a \")\" | \"w\"\n";
    let input = (0..RESTART_LEVELS).fold("wy".to_owned(), |inner, _| format!("({inner})y"));
    let parser = parser(grammar, &Options::default());
    c.bench_function(
        "a parse that starts again with every rule remembered",
        |b| {
            b.iter(|| {
                parser
                    .parse(&input, "start")
                    .expect("the input parses")
                    .len()
            })
        },
    );
}

criterion_group!(benches, passes);
criterion_main!(benches);
