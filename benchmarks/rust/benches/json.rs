// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! JSON baseline in Rust: pest's generated parser, its runtime interpreter pest_vm and the
//! hand-written serde_json, on the input of the Python JSON benchmarks.

use std::time::Duration;

use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};
use fastpeg_benchmarks::{JSON_GRAMMAR, JsonParser, RECORDS_JSON_PATH, Rule};
use pest::Parser;

/// Samples per parse benchmark. The slowest one takes about 0.3 s per iteration.
const PARSE_SAMPLES: usize = 10;

/// Time criterion measures each parse benchmark for.
const PARSE_MEASUREMENT_TIME_S: u64 = 10;

fn load_vm() -> pest_vm::Vm {
    let (_, rules) = pest_meta::parse_and_optimize(JSON_GRAMMAR).expect("the grammar is valid");
    pest_vm::Vm::new(rules)
}

fn json(c: &mut Criterion) {
    let text = std::fs::read_to_string(RECORDS_JSON_PATH)
        .expect("the input exists, run benchmarks/corpus.py first");
    let vm = load_vm();

    let mut group = c.benchmark_group("json");
    group
        .sample_size(PARSE_SAMPLES)
        .sampling_mode(SamplingMode::Flat)
        .measurement_time(Duration::from_secs(PARSE_MEASUREMENT_TIME_S))
        .throughput(Throughput::Bytes(text.len() as u64));
    group.bench_function("pest generated, token queue", |b| {
        b.iter(|| {
            JsonParser::parse(Rule::json, &text)
                .expect("the input is valid JSON")
                .flatten()
                .count()
        })
    });
    group.bench_function("pest_vm interpreter, token queue", |b| {
        b.iter(|| {
            vm.parse("json", &text)
                .expect("the input is valid JSON")
                .flatten()
                .count()
        })
    });
    group.bench_function("serde_json to Value", |b| {
        b.iter(|| {
            serde_json::from_str::<serde_json::Value>(&text).expect("the input is valid JSON")
        })
    });
    group.finish();

    c.bench_function("pest_vm grammar load", |b| b.iter(load_vm));
}

criterion_group!(benches, json);
criterion_main!(benches);
