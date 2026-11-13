// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The profile of a parse: what it counts for each rule, and that its rows account for every
//! step.

use std::path::Path;

use fpeg::{Grammar, Limits, Parser, Profile, ProfileRow};

fn profile(grammar: &str, input: &str) -> Profile {
    let grammar = Grammar::from_text(grammar).expect("the grammar reads");
    let parser = Parser::new(&grammar).expect("the grammar compiles");
    parser.profile(input, "start", Limits::default()).1
}

fn row<'p>(profile: &'p Profile, name: &str) -> &'p ProfileRow {
    let found = profile.rows.iter().find(|row| row.name == name);
    found.unwrap_or_else(|| panic!("no row {name}"))
}

#[test]
fn the_rows_of_a_profile_sum_to_the_steps_of_the_parse() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("grammars");
    // Ignored text, left recursion with the memo, a keyword policy, and an input that fails.
    for (name, sample) in [
        ("json", "nested.json"),
        ("expressions", "mixed.c"),
        ("tiny", "fib.tiny"),
        ("toml", "invalid/missing_value.toml"),
    ] {
        let grammar = std::fs::read_to_string(root.join(format!("{name}.fpeg")));
        let input = std::fs::read_to_string(root.join("samples").join(name).join(sample));
        let (grammar, input) = (grammar.expect("the grammar"), input.expect("the sample"));
        let profile = profile(&grammar, &input);
        let rows: u64 = profile.rows.iter().map(|row| row.steps).sum();
        assert!(profile.steps > input.len() as u64, "{name}: {profile:?}");
        assert_eq!(rows, profile.steps, "{name}");
        assert!(row(&profile, "(parse)").steps > 0, "{name}");
    }
}

#[test]
fn a_profile_counts_calls_and_those_that_repeat_one() {
    let grammar =
        |mark: &str| format!("start: item \".\" | item \"!\" | item\n{mark}item: \"x\"\n");
    let plain = profile(&grammar(""), "x");
    let item = row(&plain, "item");
    assert_eq!((item.calls, item.repeats, item.failures), (3, 2, 0));
    assert_eq!(
        (item.remembered, item.memo_hits, item.memo_stores),
        (false, 0, 0)
    );
    assert_eq!(row(&plain, "start").calls, 1);
    // Remembered, the two calls that repeat the first are answered from its result.
    let marked = profile(&grammar("@memo\n"), "x");
    let item = row(&marked, "item");
    assert_eq!((item.calls, item.repeats), (3, 2));
    assert_eq!(
        (item.remembered, item.memo_hits, item.memo_stores),
        (true, 2, 1)
    );
    assert_eq!(marked.memo_entries, 1);
}

#[test]
fn a_profile_counts_failed_calls_and_the_bytes_they_give_back() {
    let profile = profile("start: pair \"!\" | \"xy\"\npair: \"x\" \"z\"\n", "xy");
    let pair = row(&profile, "pair");
    // The call matched the x before it failed at the y.
    assert_eq!(
        (pair.calls, pair.failures, pair.backtracked_bytes),
        (1, 1, 1)
    );
    assert_eq!(row(&profile, "start").failures, 0);
}

#[test]
fn a_profile_counts_the_passes_that_grow_a_left_recursion() {
    let profile = profile("start: chain\nchain: chain \"-\" \"1\" | \"1\"\n", "1-1-1");
    let chain = row(&profile, "chain");
    // The call from start, and one use of the entry in each of four passes.
    assert_eq!((chain.calls, chain.growths, chain.failures), (5, 2, 0));
}

#[test]
fn a_profile_gives_the_ignored_text_a_row_of_its_own() {
    let text = "start: \"a\"+\n%ignore \" \"+\n";
    let spaced = profile(text, "a          a          a");
    let tight = profile(text, "aaa");
    assert!(row(&spaced, "%ignore").steps > row(&tight, "%ignore").steps);
    assert_eq!(row(&spaced, "start").steps, row(&tight, "start").steps);
    // The steps after a skip are the rule's again.
    let longer = profile(text, "aaaaaa");
    assert!(row(&longer, "start").steps > row(&tight, "start").steps);
}
