// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Expressions in Rust: fpeg's parsing machine on a grammar of twelve operator levels with
//! left recursion, by what it remembers. The default remembers the two levels that the level
//! above tries twice, and runs the others, which recur on their left, as loops. With nothing
//! remembered those two parse their operands again, about twenty times the work, so that
//! setting is measured on one small sum.

use std::time::Duration;

use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};
use fpeg::{Memo, Options};
use fpeg_benchmarks::{EXPRESSIONS_PATH, FPEG_EXPRESSIONS_GRAMMAR};

/// Samples per parse of the whole input, which takes about 0.1 s.
const PARSE_SAMPLES: usize = 10;

/// Time criterion measures each parse of the whole input for.
const PARSE_MEASUREMENT_TIME_S: u64 = 10;

/// The sum that every setting can parse in a time worth measuring.
const SMALL_SUM: &str = "1 + 2";

fn load(memo: Memo) -> fpeg::Parser {
    let grammar = fpeg::Grammar::from_text(FPEG_EXPRESSIONS_GRAMMAR).expect("the grammar reads");
    let options = Options {
        memo,
        ..Options::default()
    };
    fpeg::Parser::with_options(&grammar, &options).expect("the grammar compiles")
}

fn expressions(c: &mut Criterion) {
    let text = std::fs::read_to_string(EXPRESSIONS_PATH)
        .expect("the input exists, run benchmarks/corpus.py first");
    let settings = [
        ("fpeg machine, tree", Memo::Grammar),
        ("fpeg machine, every rule remembered, tree", Memo::All),
    ];

    let mut group = c.benchmark_group("expressions");
    group
        .sample_size(PARSE_SAMPLES)
        .sampling_mode(SamplingMode::Flat)
        .measurement_time(Duration::from_secs(PARSE_MEASUREMENT_TIME_S))
        .throughput(Throughput::Bytes(text.len() as u64));
    for (name, memo) in settings {
        let parser = load(memo);
        group.bench_function(name, |b| {
            b.iter(|| {
                parser
                    .parse(&text, "start")
                    .expect("the input is an expression")
                    .len()
            })
        });
    }
    group.finish();

    let small = [
        ("fpeg machine on a small sum", Memo::Grammar),
        ("fpeg machine on a small sum, nothing remembered", Memo::Off),
    ];
    for (name, memo) in small {
        let parser = load(memo);
        c.bench_function(name, |b| {
            b.iter(|| {
                parser
                    .parse(SMALL_SUM, "start")
                    .expect("the input is an expression")
                    .len()
            })
        });
    }
}

criterion_group!(benches, expressions);
criterion_main!(benches);
