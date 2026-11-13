// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rust baselines for the fastpeg benchmark suite. The benchmarks are in `benches/`.

pub use generated::{JsonParser, Rule};

/// The text of the pest JSON grammar, for the runtime interpreter pest_vm.
pub const JSON_GRAMMAR: &str = include_str!("json.pest");

/// The input shared with the Python benchmarks, written by `benchmarks/corpus.py`.
pub const RECORDS_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../corpora/records.json");

#[allow(
    missing_docs,
    reason = "pest_derive generates the rule enum and its methods without docs"
)]
mod generated {
    use pest_derive::Parser;

    /// The pest JSON grammar as a parser generated at build time.
    #[derive(Parser)]
    #[grammar = "json.pest"]
    pub struct JsonParser;
}
