// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Recovery from single edits: the bundled grammars give a broken text a tree that keeps most
//! of what the text held, scored as the share of the valid text's visible tokens, by kind and
//! text, that the recovered tree keeps, a failed parse counting none.

mod common;

use std::collections::HashMap;

use common::generator::Rng;
use common::grammars;
use fpeg::{Grammar, Parser, Tree};

/// JSON records made the way the benchmark's are, and the edits of each.
const RECORDS: u64 = 30;
const EDITS_PER_RECORD: usize = 20;

/// What an edit inserts, or puts in place of a character: JSON's punctuation, other punctuation,
/// layout, letters and digits.
const INSERTED: [char; 25] = [
    '(', ')', '[', ']', '{', '}', ':', ',', '.', '=', '+', '-', '*', '/', '"', '\'', ' ', '\n',
    '#', 'a', 'b', 'c', '0', '1', '9',
];

/// The shares that the JSON grammar keeps, set from its first measurement on 2026-11-15, 294 of
/// 297 broken texts with a tree and 93.2 percent of the visible tokens kept.
const JSON_MIN_TREES_PERCENT: f64 = 98.0;
const JSON_MIN_KEPT_PERCENT: f64 = 93.0;

/// The parser of the bundled JSON grammar.
fn json() -> Parser {
    let text = std::fs::read_to_string(grammars().join("json.fpeg")).expect("it is readable");
    Parser::new(&Grammar::from_text(&text).expect("reads")).expect("compiles")
}

/// A record as the benchmark makes them, written with one space for each level.
fn record(i: u64, rng: &mut Rng) -> String {
    const WORDS: [&str; 6] = ["alpha", "beta", "resistor", "nmos", "net", "cell"];
    let tags: Vec<String> = (0..3)
        .map(|_| format!("  \"{}\"", rng.pick(&WORDS)))
        .collect();
    let parent = match i % 7 {
        0 => "null".to_owned(),
        _ => (i / 2).to_string(),
    };
    format!(
        "{{\n \"id\": {i},\n \"name\": \"{}_{i}\",\n \"enabled\": {},\n \"ratio\": {}.{:04},\n \
         \"tags\": [\n{}\n ],\n \"pos\": {{\n  \"x\": {},\n  \"y\": -{}\n }},\n \
         \"parent\": {parent},\n \"note\": \"line one\\nline \\\"two\\\" \\u00e4\"\n}}",
        rng.pick(&WORDS),
        rng.below(2) == 0,
        rng.below(1000),
        rng.below(10_000),
        tags.join(",\n"),
        rng.below(5000),
        rng.below(5000),
    )
}

/// The text with one character deleted, inserted or replaced.
fn edit(text: &str, rng: &mut Rng) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    let at = rng.below(chars.len() as u64) as usize;
    match rng.below(3) {
        0 => {
            chars.remove(at);
        }
        1 => chars.insert(at, *rng.pick(&INSERTED)),
        _ => chars[at] = *rng.pick(&INSERTED),
    }
    chars.into_iter().collect()
}

/// Each visible token of a tree, by kind and text, with how often it stands there.
fn visible(tree: &Tree, input: &str) -> HashMap<(String, String), usize> {
    let mut counts = HashMap::new();
    for item in (0..tree.len()).filter_map(|index| tree.node(index)) {
        if item.is_token() && !item.is_hidden() && !item.kind_name().starts_with('<') {
            let key = (item.kind_name().to_owned(), item.text(input).to_owned());
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    counts
}

#[test]
fn the_json_grammar_recovers_most_of_a_broken_text() {
    let parser = json();
    let mut rng = Rng::seeded(8);
    let (mut broken, mut trees, mut kept) = (0, 0, 0.0);
    for i in 0..RECORDS {
        let valid = record(i * 30, &mut rng);
        let tree = parser.parse(&valid, "start").expect("a record parses");
        let again = parser
            .parse_recovering(&valid, "start")
            .expect("and with recovery");
        assert_eq!(tree.to_lines(&valid, true), again.to_lines(&valid, true));
        let want = visible(&tree, &valid);
        let total: usize = want.values().sum();
        for _ in 0..EDITS_PER_RECORD {
            let edited = edit(&valid, &mut rng);
            if parser.parse(&edited, "start").is_ok() {
                continue;
            }
            broken += 1;
            let Ok(recovered) = parser.parse_recovering(&edited, "start") else {
                continue;
            };
            trees += 1;
            let got = visible(&recovered, &edited);
            let same: usize = (want.iter())
                .map(|(key, &n)| n.min(got.get(key).copied().unwrap_or(0)))
                .sum();
            kept += same as f64 / total as f64;
        }
    }
    let trees_percent = 100.0 * trees as f64 / broken as f64;
    let kept_percent = 100.0 * kept / broken as f64;
    println!("{trees} of {broken} broken texts give a tree, {kept_percent:.1} % of tokens kept");
    assert!(broken > 200, "{broken} broken texts");
    assert!(
        trees_percent >= JSON_MIN_TREES_PERCENT,
        "{trees_percent:.1} % give a tree"
    );
    assert!(
        kept_percent >= JSON_MIN_KEPT_PERCENT,
        "{kept_percent:.1} % of tokens kept"
    );
}

#[test]
fn a_json_member_that_does_not_parse_skips_whole_values_up_to_any_closing_bracket() {
    let parser = json();
    let labels = |input: &str| {
        let tree = parser.parse_recovering(input, "start").expect("recovers");
        let labels: Vec<String> = tree.recoveries().iter().map(|r| r.label.clone()).collect();
        labels
    };
    assert_eq!(labels("{\"a\": 1, [2, 3], \"b\": 4}"), ["member"]);
    assert_eq!(labels("[{\"a\": 1, ]"), ["member", "brace"]);
}

#[test]
fn the_json_grammar_names_each_mistake_and_what_it_expected() {
    let parser = json();
    let message = |input: &str| {
        let error = parser.parse(input, "start").expect_err("a mistake");
        format!("{} at {}", error.message(), error.position)
    };
    assert_eq!(message("[1, 2"), "bracket: expected \",\", \"]\" at 5");
    assert_eq!(
        message("[1,]"),
        "value: expected \"[\", \"false\", \"null\", \"true\", \"{\", number, string at 3"
    );
    assert_eq!(message("{\"a\" 1}"), "colon: expected \":\" at 5");
    assert_eq!(message("[1] x"), "trailing: expected end of input at 4");
}
