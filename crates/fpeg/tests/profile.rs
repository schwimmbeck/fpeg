// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The profile of a parse: what it counts for each rule, and that its rows account for every
//! step.

mod common;

use common::{RESTART_MIN_STEPS, grammars, pathological_input};
use fpeg::{Grammar, Limits, Options, Parser, Passes, Profile, ProfileRow};

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
    let root = grammars();
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
fn a_grammar_without_left_recursion_profiles_alike_whatever_passes_the_parser_runs() {
    let grammar = Grammar::from_text("start: WORD+\nWORD: [a-z]+\n%ignore \" \"+\n");
    let grammar = grammar.expect("the grammar reads");
    let [optimized, plain] = [Passes::ALL, Passes::NONE].map(|passes| {
        let options = Options {
            passes,
            ..Options::default()
        };
        let parser = Parser::with_options(&grammar, &options).expect("the grammar compiles");
        parser.profile("ab  cd ef", "start", Limits::default()).1
    });
    assert_eq!(optimized, plain);
}

#[test]
fn a_profile_remembers_the_rules_that_the_parse_remembers() {
    // `b` runs as a loop in the parse, which remembers it as little as any rule that the
    // alternatives of two rules start with, so that its second call repeats the first.
    let text = "start: p | q\np: b \"x\"\nq: b \"y\"\nb: b \"+\" | \"n\"\n";
    let profile = profile(text, "n+y");
    let b = row(&profile, "b");
    assert_eq!((b.remembered, b.repeats, b.memo_hits), (false, 1, 0));
}

#[test]
fn a_profile_counts_both_runs_of_a_parse_that_started_again() {
    // `b` is matched twice at every level, which nothing remembered by default saves, until
    // the parse runs out of its budget and starts again with every rule remembered.
    let grammar = "start: a\na: c | d\nc: b \"x\"\nd: b \"y\"\nb: \"(\" a \")\" | \"w\"\n";
    let profile = profile(grammar, &pathological_input());
    assert_eq!(profile.restarts, 1);
    assert!(profile.steps > RESTART_MIN_STEPS, "{}", profile.steps);
    let rows: u64 = profile.rows.iter().map(|row| row.steps).sum();
    assert_eq!(rows, profile.steps);
    // The call of start that the first run left open counts to where that run ended.
    let (start, around) = (row(&profile, "start"), row(&profile, "(parse)"));
    assert_eq!(start.steps_with_callees, profile.steps - around.steps);
    assert!(!row(&profile, "b").remembered);
}

#[test]
fn a_profile_counts_calls_and_those_that_repeat_one() {
    let grammar =
        |mark: &str| format!("start: item \".\" | item \"!\" | item\n{mark}item: \"x\"\n");
    let plain = profile(&grammar("@nomemo\n"), "x");
    let item = row(&plain, "item");
    assert_eq!((item.calls, item.repeats, item.failures), (3, 2, 0));
    assert_eq!(
        (item.remembered, item.memo_hits, item.memo_stores),
        (false, 0, 0)
    );
    assert_eq!(row(&plain, "start").calls, 1);
    // The calls cost alike, so the two that repeat the first spend two thirds of the steps.
    assert_eq!(item.repeated_steps * 3, item.steps_with_callees * 2);
    // Where a rule repeats inside its own repeats, the outermost counts, as for its steps.
    let nested = "start: dot | bang | item\ndot: item \".\"\nbang: item \"!\"\n\
                  item: \"(\" start \")\" | \"x\"\n";
    let nested = profile(nested, "((((x))))");
    assert!(row(&nested, "item").repeated_steps > 0);
    for row in &nested.rows {
        assert!(row.repeated_steps <= row.steps_with_callees, "{row:?}");
    }
    // Remembered, the two calls that repeat the first are answered from its result.
    let marked = profile(&grammar("@memo\n"), "x");
    let item = row(&marked, "item");
    assert_eq!((item.calls, item.repeats, item.repeated_steps), (3, 2, 0));
    assert_eq!(
        (item.remembered, item.memo_hits, item.memo_stores),
        (true, 2, 1)
    );
    assert_eq!(marked.memo_entries, 1);
    // A remembered failure answers too.
    let failing = profile(&grammar("@memo\n"), "y");
    let item = row(&failing, "item");
    assert_eq!((item.calls, item.memo_hits, item.memo_stores), (3, 2, 1));
}

#[test]
fn a_profile_counts_failed_calls_and_the_bytes_they_give_back() {
    let pairs = profile("start: pair \"!\" | \"xy\"\npair: \"x\" \"z\"\n", "xy");
    let pair = row(&pairs, "pair");
    // The call matched the x before it failed at the y.
    assert_eq!(
        (pair.calls, pair.failures, pair.backtracked_bytes),
        (1, 1, 1)
    );
    assert_eq!(row(&pairs, "start").failures, 0);
    // Everything but the code around the parse happens inside the call of start.
    let (start, around) = (row(&pairs, "start"), row(&pairs, "(parse)"));
    assert_eq!(start.steps_with_callees, pairs.steps - around.steps);
    assert_eq!(start.steps_with_callees, start.steps + pair.steps);
    // The first try got through two bytes, the last none.
    let items = profile(
        "start: item | \"abd\"\nitem: \"a\" \"b\" \"c\" | \"x\"\n",
        "abd",
    );
    let item = row(&items, "item");
    assert_eq!((item.failures, item.backtracked_bytes), (1, 2));
}

#[test]
fn a_profile_counts_the_steps_of_a_template_that_calls_itself_once() {
    let text = "start: t<\"a\">\nt<p>: p | \"(\" t<\"b\"> \")\" | \"[\" t<p> \"]\"\n";
    for input in ["(b)", "[(b)]"] {
        let profile = profile(text, input);
        let (start, t) = (row(&profile, "start"), row(&profile, "t"));
        // The instances of t share a row, and the outermost call holds the inner ones.
        assert_eq!(
            start.steps_with_callees,
            start.steps + t.steps_with_callees,
            "{input}"
        );
    }
}

#[test]
fn a_profile_runs_the_levels_of_a_precedence_block_as_the_parse_does() {
    // Kept as the growing loop with nothing remembered, each level would grow again in every
    // pass of the level above, until the parse started again with every rule remembered. So
    // the levels run as loops where the parse runs them so, and are remembered where not.
    let root = grammars();
    let grammar = std::fs::read_to_string(root.join("expressions.fpeg"));
    let input = std::fs::read_to_string(root.join("samples").join("expressions").join("mixed.c"));
    let grammar = Grammar::from_text(&grammar.expect("the grammar")).expect("it reads");
    let input = input.expect("the sample");
    for left_loop in [true, false] {
        let options = Options {
            passes: Passes {
                left_loop,
                ..Passes::ALL
            },
            ..Options::default()
        };
        let parser = Parser::with_options(&grammar, &options).expect("the grammar compiles");
        let profile = parser.profile(&input, "start", Limits::default()).1;
        assert_eq!(profile.restarts, 0, "{left_loop}: {} steps", profile.steps);
    }
}

#[test]
fn a_profile_counts_the_passes_that_grow_a_left_recursion() {
    // Run as a loop, as in the parse, the rule is called once.
    let looped = profile("start: chain\nchain: chain \"-\" \"1\" | \"1\"\n", "1-1-1");
    let chain = row(&looped, "chain");
    assert_eq!(
        (chain.calls, chain.repeats, chain.growths, chain.failures),
        (1, 0, 2, 0)
    );
    // Through another rule it keeps the growing loop: the call from start, and one use of the
    // entry in each of four passes, which repeats nothing that remembering the rule would
    // answer.
    let text = "start: chain\nchain: link \"1\" | \"1\"\nlink: chain \"-\"\n";
    let growing = profile(text, "1-1-1");
    let chain = row(&growing, "chain");
    assert_eq!(
        (chain.calls, chain.repeats, chain.growths, chain.failures),
        (5, 0, 2, 0)
    );
}

#[test]
fn a_profile_gives_the_ignored_text_a_row_of_its_own() {
    // Also where the skip routine calls a terminal, which returns to the routine.
    for text in [
        "start: \"a\"+\n%ignore \" \"+\n",
        "start: \"a\"+\n%ignore WS\nWS: \" \"\n",
    ] {
        let spaced = profile(text, "a          a          a");
        let tight = profile(text, "aaa");
        assert!(row(&spaced, "%ignore").steps > row(&tight, "%ignore").steps);
        assert_eq!(
            row(&spaced, "start").steps,
            row(&tight, "start").steps,
            "{text}"
        );
        // The steps after a skip are the rule's again.
        let longer = profile(text, "aaaaaa");
        assert!(row(&longer, "start").steps > row(&tight, "start").steps);
    }
    // Also after an error of the skip routine that a lookahead caught.
    let text = "start: !(\"a\" \"b\") rest \"y\"*\n@lexical\nrest: \"a/!\"\n\
                %ignore \"/\" (\"/\" ^ \"slash\")\n";
    let (short, long) = (profile(text, "a/!yy"), profile(text, "a/!yyyyyyyy"));
    assert!(row(&long, "start").steps > row(&short, "start").steps);
}
