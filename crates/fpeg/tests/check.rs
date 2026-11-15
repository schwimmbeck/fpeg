// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The warnings of `check`: each is given where it holds and left out where it does not. The
//! pages of the codes show one grammar for each, which `tests/diagnostics.rs` holds true.

use std::path::Path;
use std::time::Instant;

use fpeg::builder::{GrammarBuilder, ch, class, lit, many, opt, r, seq};
use fpeg::{Expr, ExprKind, Grammar};

/// The codes `check` gives for a grammar text, in the order of their places.
fn codes(text: &str) -> Vec<&'static str> {
    let grammar = Grammar::from_text(text).unwrap_or_else(|errors| panic!("{errors:?}"));
    let found = fpeg::check(&grammar);
    found
        .iter()
        .map(|diagnostic| diagnostic.code.code())
        .collect()
}

#[test]
fn the_acceptance_grammars_give_no_error_and_no_warning() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("grammars");
    let mut checked = 0;
    for entry in std::fs::read_dir(&root).expect("the grammars directory exists") {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_none_or(|extension| extension != "fpeg") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("the grammar is readable");
        let grammar = Grammar::from_text(&text).expect("the grammar reads");
        let found = fpeg::check(&grammar);
        assert!(found.is_empty(), "{}: {found:?}", path.display());
        checked += 1;
    }
    assert!(checked >= 10, "{checked} grammars");
}

#[test]
fn a_grammar_that_does_not_resolve_gives_its_errors_alone() {
    assert_eq!(codes("start: missing | \"a\"? | \"b\"\n"), ["E201"]);
    // In the order of the text, as all that `check` gives.
    assert_eq!(
        codes("start: b a\nstart: \"x\"\nc: zz\n"),
        ["E201", "E201", "E204", "E201"]
    );
}

#[test]
fn an_alternative_after_one_that_cannot_fail_is_never_tried() {
    for (text, expected) in [
        ("start: \"a\"? | \"b\"\n", vec!["W101"]),
        ("start: \"a\"* | \"b\" | \"c\"\n", vec!["W101"]),
        ("start: () | \"b\"\n", vec!["W101"]),
        // A labeled expression matches or ends the parse.
        ("start: \"a\" ^ \"an a\" | \"b\"\n", vec!["W101"]),
        ("start: x=(\"a\" ** \",\") | \"b\"\n", vec!["W101"]),
        // Inside a group, and in the ignored text.
        ("start: (\"a\"? | \"b\") \"c\"\n", vec!["W101"]),
        (
            "start: \"a\"\n%ignore \" \"*\n%ignore \"#\"\n",
            vec!["W101"],
        ),
        // A cut inside a choice of its own commits only that choice.
        ("start: (\"a\" | ~ \"b\")? | \"c\"\n", vec!["W101"]),
        // These can fail.
        ("start: \"a\"+ | \"b\"\n", vec![]),
        ("start: \"a\" ++ \",\" | \"b\"\n", vec![]),
        ("start: !\"a\" | \"b\"\n", vec![]),
        ("start: \"b\" | \"a\"?\n", vec![]),
        // A failure after a cut fails the repetition or ends the choice around it.
        ("start: (\"a\" ~ \"b\")? | \"a\" \"c\"\n", vec![]),
        ("start: (\"a\" ~ \"b\")* | \"a\" \"c\"\n", vec![]),
        ("start: (\"a\" ~ \"b\") ** \",\" | \"a\" \"c\"\n", vec![]),
        ("start: (\"a\" ~ \"b\" | ()) | \"a\" \"c\"\n", vec![]),
        // An argument can hold a cut.
        (
            "start: many<\"a\" ~ \"b\">\nmany<item>: item* -> items | \"a\" \"c\" -> ac\n",
            vec![],
        ),
    ] {
        assert_eq!(codes(text), expected, "{text}");
    }
}

#[test]
fn a_level_whose_last_alternative_cannot_fail_cuts_off_the_levels_after_it() {
    let block = |sign: &str| {
        format!("start: e\ne: %precedence {{\n    signed: {sign} -> signed\n    atom: \"1\"\n}}\n")
    };
    assert_eq!(codes(&block("\"-\"? atom")), Vec::<&str>::new());
    assert_eq!(codes(&block("\"-\"?")), ["W101"]);
    assert_eq!(codes(&block("(\"-\" ~ atom)?")), Vec::<&str>::new());
}

#[test]
fn a_literal_is_shadowed_where_the_earlier_one_matches_whenever_it_would() {
    for (text, expected) in [
        ("start: \"<\" | \"<=\"\n", vec!["W102"]),
        ("start: \"<=\" | \"<\"\n", vec![]),
        ("start: x=\"=\" | y=\"==\"\n", vec!["W102"]),
        ("start: \"a\" | \"b\" | \"c\" | \"ab\"\n", vec!["W102"]),
        // A literal that ignores case matches every spelling of the later one too.
        ("start: \"in\"i | \"INNER\"\n", vec!["W102"]),
        ("start: \"a\"i | \"A\"i\n", vec!["W102"]),
        ("start: \"in\" | \"inner\"i\n", vec![]),
        // With word characters set, a literal of a rule fails in front of one.
        ("start: \"in\" | \"inner\"\n%word [a-z]\n", vec![]),
        ("start: \"in\" | \"in+\"\n%word [a-z]\n", vec!["W102"]),
        // A later literal that ignores case goes on with a word character in either case.
        ("start: \"in\"i | \"inNer\"i\n%word [a-z]\n", vec![]),
        // One as long that ends in no word character, as written, has no boundary of its own.
        ("start: (\"a\"i | \"A\"i) \"b\"\n%word [a-z]\n", vec![]),
        ("start: (\"in\"i | \"IN\") \"x\"\n%word [a-z]\n", vec![]),
        ("start: \"+\" | \"+=\"\n%word [a-z]\n", vec!["W102"]),
        // Inside a terminal no boundary holds.
        (
            "start: T\nT: \"in\" | \"inner\"\n%word [a-z]\n",
            vec!["W102"],
        ),
    ] {
        assert_eq!(codes(text), expected, "{text}");
    }
}

#[test]
fn an_alternative_is_shadowed_by_an_earlier_one_it_starts_with() {
    for (text, expected) in [
        (
            "start: A B | A B C\nA: \"a\"\nB: \"b\"\nC: \"c\"\n",
            vec!["W103"],
        ),
        ("start: A | A\nA: \"a\"\n", vec!["W103"]),
        // Fields and aliases change the tree and not the match.
        (
            "start: x=A B -> one | A y=B C -> two\nA: \"a\"\nB: \"b\"\nC: \"c\"\n",
            vec!["W103"],
        ),
        (
            "start: A* B | A* B C\nA: \"a\"\nB: \"b\"\nC: \"c\"\n",
            vec!["W103"],
        ),
        // The longer one first, or a difference before the end, is no shadow.
        ("start: A B C | A B\nA: \"a\"\nB: \"b\"\nC: \"c\"\n", vec![]),
        ("start: A B | A C B\nA: \"a\"\nB: \"b\"\nC: \"c\"\n", vec![]),
        (
            "start: A+ B | A* B C\nA: \"a\"\nB: \"b\"\nC: \"c\"\n",
            vec![],
        ),
        ("start: A !B | A B\nA: \"a\"\nB: \"b\"\n", vec![]),
        // Labels written alike, wherever they stand.
        (
            "start: \"a\" ^ \"an a\" B | \"a\" ^ \"an a\" B C\nB: \"b\"\nC: \"c\"\n",
            vec!["W103"],
        ),
        // Flags and their tests, classes, separators and any character, written alike.
        (
            "%flag f\nstart: %on(f, %if(f) [a-z] ** \",\") %off(f, %if(!f) .)\n\
             | %on(f, %if(f) [a-z] ** \",\") %off(f, %if(!f) .) \"!\"\n",
            vec!["W103"],
        ),
    ] {
        assert_eq!(codes(text), expected, "{text}");
    }
}

/// Time the check of a choice between thousands of alternatives may take in a debug build.
/// Each is compared with the 256 before it, by a hash of each part first, which takes a second
/// or two. All pairs take a minute, and pairs compared down to the bottom of a deep first part
/// they share a quarter of a minute.
const LONG_CHOICE_SECONDS: f64 = 10.0;

#[test]
fn a_choice_between_thousands_of_alternatives_is_checked_in_time() {
    let distinct = (0..30_000).map(|i| seq([lit(&format!("k{i:05}")), r("X")]));
    // A first part ninety levels deep that all alternatives share, then one of their own.
    let deep = |bottom: Expr| (0..90).fold(bottom, |part, _| opt(part));
    let shared = (0..4_000).map(|i| seq([deep(r("X")), lit(&format!("k{i:05}"))]));
    // A first part as deep, whose class at the bottom is the only difference.
    let symbol = |i: u32| char::from_u32(0x4E00 + i).expect("a code point");
    let classes = (0..4_000).map(|i| seq([deep(class([ch(symbol(i))])), r("X")]));
    for alternatives in [
        distinct.collect::<Vec<_>>(),
        shared.collect(),
        classes.collect(),
    ] {
        let mut g = GrammarBuilder::new();
        g.rule("start", Expr::new(ExprKind::Choice(alternatives)));
        g.rule("X", lit("x"));
        let grammar = g.build();
        let started = Instant::now();
        assert!(fpeg::check(&grammar).is_empty());
        let seconds = started.elapsed().as_secs_f64();
        assert!(seconds < LONG_CHOICE_SECONDS, "{seconds:.1} s");
    }
}

/// Time the check of 30,000 rules may take in a debug build, each of which can match nothing
/// only by way of the one before it. Working a rule out once takes well under a second. Working
/// it out again for every rule it calls takes minutes.
const LONG_CHAIN_SECONDS: f64 = 20.0;

#[test]
fn a_chain_of_thousands_of_rules_that_match_nothing_is_checked_in_time() {
    let calls = (0..30_000).map(|i| r(&format!("r{i}")));
    let mut g = GrammarBuilder::new();
    g.rule(
        "start",
        many(Expr::new(ExprKind::Sequence(calls.collect()))),
    );
    g.rule("r0", opt(lit("a")));
    for i in 1..30_000 {
        g.rule(&format!("r{i}"), r(&format!("r{}", i - 1)));
    }
    let grammar = g.build();
    let started = Instant::now();
    let found = fpeg::check(&grammar);
    let seconds = started.elapsed().as_secs_f64();
    let codes: Vec<_> = found
        .iter()
        .map(|diagnostic| diagnostic.code.code())
        .collect();
    assert_eq!(codes, ["W201"]);
    assert!(seconds < LONG_CHAIN_SECONDS, "{seconds:.1} s");
}

#[test]
fn a_repetition_of_what_can_match_nothing_ends_at_an_empty_step() {
    for (text, expected) in [
        ("start: (\"a\"?)* \"b\"\n", vec!["W201"]),
        ("start: (\"a\"? \"b\"*)+ \"c\"\n", vec!["W201"]),
        ("start: (\"a\"*){2} \"c\"\n", vec!["W201"]),
        ("start: (\"a\"? ** (\",\"?)) \"c\"\n", vec!["W201"]),
        // Through rules, where one can match nothing only by way of another.
        (
            "start: item* \"c\"\nitem: part\npart: \"a\"?\n",
            vec!["W201"],
        ),
        // Through a precedence block, whose first level steps on to one that can match nothing.
        (
            "start: e* \"c\"\ne: %precedence {\n    signed: \"-\" atom -> signed\n    atom: \"1\"?\n}\n",
            vec!["W201"],
        ),
        // A part or a separator that must match makes every step consume.
        ("start: (\"a\"? \"b\")* \"c\"\n", vec![]),
        ("start: (\"a\"? ** \",\") \"c\"\n", vec![]),
        // An optional expression is no repetition.
        ("start: (\"a\"*)? \"c\"\n", vec![]),
        // What looks ahead or tests a flag is not known to match nothing.
        ("start: (!\"b\" \"a\"?)* \"b\"\n", vec![]),
        ("start: %on(f, (%if(f) \"a\"?)* \"b\")\n%flag f\n", vec![]),
        // Where it fails, a later alternative that can match nothing does.
        ("start: (&\"a\" \"a\" | \"b\"?)* \"c\"\n", vec!["W201"]),
        // No alternative is tried after one that ends the parse where it does not match.
        ("start: (\"a\" ^ \"an a\" | \"b\"?)* \"c\"\n", vec!["W101"]),
        (
            "start: item* \"c\"\nitem: \"a\" ^ \"an a\" | \"b\"?\n",
            vec!["W101"],
        ),
        // Behind a cut or an error label, an alternative is reached only where the one
        // before it fails without passing the cut or failing the label.
        ("start: (\"a\"? ~ \"b\" | ()){2} \"c\"\n", vec![]),
        ("start: (\"a\"? ~ \"b\" | ())* \"c\"\n", vec![]),
        ("start: ((\"b\" ^ \"a b\")?)* \"c\"\n", vec![]),
        // Also a label in a rule that the alternative calls, in a level that a block starts
        // with, or in the level that a level falls through to.
        (
            "start: item* \"c\"\nitem: thing | \"b\"?\nthing: \"a\" ^ \"an a\"\n",
            vec![],
        ),
        (
            "start: item* \"c\"\nitem: e | \"b\"?\ne: %precedence {\n    \
             neg: \"-\" \"1\" -> neg\n    atom: \"1\" ^ \"a one\"\n}\n",
            vec![],
        ),
        // A skip expression that can fail a label may do so before any token, so only the
        // repetitions of terminals, which do not skip, are reported.
        (
            "start: (\"a\"?)* T\nT: (\"b\"?)* \"c\"\n\
             %ignore \" \" | \"/*\" (!\"*/\" .)* (\"*/\" ^ \"the end of a comment\")\n",
            vec!["W201"],
        ),
        // A cut inside a choice of its own, or a label inside a lookahead, stops nothing.
        ("start: ((\"a\" | ~ \"b\") | \"c\"?)* \"d\"\n", vec!["W201"]),
        (
            "start: (!(\"a\" ^ \"an a\") \"b\" | \"c\"?)* \"d\"\n",
            vec!["W201"],
        ),
        // A parameter stands for any expression, also under the name of a rule.
        ("start: many<\"a\"?> \"b\"\nmany<x>: x*\n", vec![]),
        (
            "start: many<\"a\"> item \"b\"\nmany<item>: item*\nitem: \"a\"?\n",
            vec![],
        ),
    ] {
        assert_eq!(codes(text), expected, "{text}");
    }
}

#[test]
fn a_rule_that_calls_itself_at_both_ends_of_an_alternative_groups_to_the_right() {
    for (text, expected) in [
        ("start: e\ne: e \"-\" e | \"1\"\n", vec!["W202"]),
        (
            "start: e\ne: left=e \"-\" right=e -> minus | \"1\"\n",
            vec!["W202"],
        ),
        (
            "start: x\nx: %precedence {\n    sum: sum \"+\" sum -> add\n    atom: \"1\"\n}\n",
            vec!["W202"],
        ),
        // A template that calls itself with its own parameters.
        (
            "start: list<N>\nlist<x>: list<x> \"-\" list<x> -> minus | x\nN: [0-9]+\n",
            vec!["W202"],
        ),
        // A parameter of the name of its rule stands for the argument.
        (
            "start: pair<N>\npair<pair>: pair \"-\" pair\nN: [0-9]+\n",
            vec![],
        ),
        // At one end only.
        ("start: e\ne: e \"-\" \"1\" | \"1\"\n", vec![]),
        ("start: e\ne: \"1\" \"-\" e | \"1\"\n", vec![]),
        ("start: e\ne: \"(\" e \")\" | e \"!\" | \"1\"\n", vec![]),
    ] {
        assert_eq!(codes(text), expected, "{text}");
    }
}

#[test]
fn an_alternative_of_a_transparent_rule_makes_a_node_for_every_input_or_for_none() {
    for (body, expected) in [
        ("N (\"+\" N)*", vec!["W301"]),
        ("N?", vec!["W301"]),
        ("N*", vec!["W301"]),
        // Through a rule that makes no node of its own.
        ("_items\n_items: N N?", vec!["W301"]),
        ("_items\n_items: N N | N", vec!["W301"]),
        // A literal leaves a hidden token, which does not count.
        ("\"(\" N \")\"", vec![]),
        ("N N", vec![]),
        // None or two: the node stays either way.
        ("(N N)?", vec![]),
        ("(N N?){2}", vec![]),
        // An alternative with an alias always makes its node.
        ("N (\"+\" N)* -> sum", vec![]),
        ("N (\"+\" N)+ -> sum | N", vec![]),
    ] {
        let text = format!("start: x\n?x: {body}\nN: [0-9]+\n");
        assert_eq!(codes(&text), expected, "{text}");
    }
    // What a rule with parameters leaves depends on its arguments.
    assert_eq!(
        codes("start: maybe<N>\n?maybe<x>: x?\nN: [0-9]+\n"),
        Vec::<&str>::new()
    );
}

#[test]
fn a_builder_is_unused_on_an_alternative_that_always_gives_way() {
    for (body, expected) in [
        ("N => list", vec!["W302"]),
        ("\"(\" N \")\" => list | N N => list", vec!["W302"]),
        // The node stays, or the alias makes one.
        ("N N => list", vec![]),
        ("N -> one => list", vec![]),
        // Where the shape depends on the input, that is the warning.
        ("N* => list", vec!["W301"]),
    ] {
        let text = format!("start: x\n?x: {body}\nN: [0-9]+\n");
        assert_eq!(codes(&text), expected, "{text}");
    }
}

#[test]
fn a_flag_is_set_somewhere_and_tested_somewhere() {
    for (text, expected) in [
        ("start: %on(f, \"a\")\n%flag f\n", vec!["W401"]),
        ("start: %if(f) \"a\"\n%flag f\n", vec!["W401"]),
        ("start: \"a\"\n%flag f\n", vec!["W401"]),
        // Clearing a flag that nothing sets changes nothing.
        ("start: %off(f, %if(!f) \"a\")\n%flag f\n", vec!["W401"]),
        // Each flag on its own.
        ("start: %on(f, %if(f) \"a\")\n%flag f g\n", vec!["W401"]),
        // Set in one rule and tested in another, or in the ignored text.
        ("start: %on(f, item)\nitem: %if(f) \"a\"\n%flag f\n", vec![]),
        (
            "start: %on(f, \"a\" \"b\")\n%ignore \" \" | %if(f) \"\\n\"\n%flag f\n",
            vec![],
        ),
        // Or in a recovery.
        (
            "start: %on(f, \"a\" ^ l)\n%recover l: %if(f) \"b\"\n%flag f\n",
            vec![],
        ),
    ] {
        assert_eq!(codes(text), expected, "{text}");
    }
}

#[test]
fn a_rule_uses_each_of_its_parameters() {
    for (text, expected) in [
        ("start: pair<\"a\", \"b\">\npair<x, y>: x x\n", vec!["W402"]),
        ("start: pair<\"a\", \"b\">\npair<x, y>: x y\n", vec![]),
        // Handing a parameter on is a use.
        (
            "start: twice<\"a\">\ntwice<x>: pair<x, x>\npair<x, y>: x y\n",
            vec![],
        ),
        // A rule of the same name as a parameter of another rule is no use of it.
        (
            "start: one<\"a\"> x\none<x>: \"b\"\nx: \"c\"\n",
            vec!["W402"],
        ),
    ] {
        assert_eq!(codes(text), expected, "{text}");
    }
}
