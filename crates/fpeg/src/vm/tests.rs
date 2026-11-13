// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the compiler, the verifier and the machine that need their insides. What a parse
//! returns is compared with the reference interpreter in the crate's differential tests.

use super::machine::Machine;
use super::program::{Class, Inst, Program};
use super::verify::verify;
use super::{Memo, Options, Parser};
use crate::builder::{GrammarBuilder, lit, opt, r, r_args, sep_many, seq};
use crate::grammar::{ClassItem, Expr, ExprKind, Grammar, MAX_EXPRESSION_DEPTH, Property};
use crate::ir::CharSet;
use crate::parse::{Limit, Limits, ParseErrorKind};

fn parser(text: &str) -> Parser {
    let grammar = Grammar::from_text(text).unwrap_or_else(|e| panic!("{e:?}"));
    Parser::new(&grammar).unwrap_or_else(|e| panic!("{e:?}"))
}

fn errors(text: &str) -> Vec<String> {
    let grammar = Grammar::from_text(text).unwrap_or_else(|e| panic!("{e:?}"));
    match Parser::new(&grammar) {
        Ok(_) => Vec::new(),
        Err(errors) => errors.into_iter().map(|e| e.message).collect(),
    }
}

#[test]
fn parsers_and_trees_cross_threads_and_entries_read_by_position() {
    fn shared<T: Send + Sync>() {}
    shared::<Parser>();
    shared::<crate::Tree>();
    let parser = parser(
        "start: A \"+\" A
A: \"a\"",
    );
    let tree = parser
        .parse("a+a", "start")
        .unwrap_or_else(|e| panic!("{e:?}"));
    // Postorder: the two tokens A and the hidden "+" come before their node.
    let root = tree
        .node(tree.len() - 1)
        .unwrap_or_else(|| panic!("a root"));
    assert_eq!(
        (root.index(), root.size(), root.kind_name()),
        (3, 4, "start")
    );
    assert_eq!(tree.node(1).map(|plus| plus.is_hidden()), Some(true));
    assert!(tree.node(4).is_none());
}

#[test]
fn a_failed_alternative_gives_back_the_nodes_it_made() {
    // The first alternative builds the whole nested tree and then fails, at every level.
    let parser = parser("start: s\ns: \"(\" s \")\" \"x\" | \"(\" s \")\" | \"a\"\n");
    let input = format!("{}a{}", "(".repeat(12), ")".repeat(12));
    let mut machine = Machine::new(&parser.program, &input, Limits::default(), None);
    assert_eq!(machine.run(parser.program.entries["start"]), Ok(()));
    let tree = machine.tree().expect("the tree fits");
    // Keeping them would hold the nodes of every one of the 2^12 failed attempts.
    assert!(
        machine.arena_len() <= tree.len(),
        "{} nodes in the arena for a tree of {}",
        machine.arena_len(),
        tree.len()
    );
}

#[test]
fn an_instruction_takes_at_most_twelve_bytes() {
    assert!(size_of::<Inst>() <= 12, "{}", size_of::<Inst>());
}

#[test]
fn rules_compile_to_their_templates_and_shared_items_to_one_routine() {
    let program = parser(concat!(
        "start: pair\n",
        "pair: key=STRING \":\" value=value\n",
        "value: STRING | \"[\" (value ** \",\") \"]\"\n",
        "STRING: \"'\" [^']* \"'\"\n",
        "%ignore \" \"+\n",
    ))
    .program;
    let expected = "\
rule start:
     0  Open
     1  Call pair
     2  CloseNode start
     3  Ret
rule pair:
     4  Open
     5  Open
     6  Skip
     7  TokenOpen STRING
     8  Call STRING
     9  TokenClose
    10  CloseField key
    11  Skip
    12  TokenOpen \":\"
    13  Char ':'
    14  TokenClose hidden
    15  Open
    16  Call value
    17  CloseField value
    18  CloseNode pair
    19  Ret
rule value:
    20  Choice 28
    21  Open
    22  Skip
    23  TokenOpen STRING
    24  Call STRING
    25  TokenClose
    26  CloseNode value
    27  Commit 48
    28  Open
    29  Skip
    30  TokenOpen \"[\"
    31  Char '['
    32  TokenClose hidden
    33  Choice 43
    34  Sub shared 0
    35  Choice 42
    36  Skip
    37  TokenOpen \",\"
    38  Char ','
    39  TokenClose hidden
    40  Sub shared 0
    41  LoopCommit 36
    42  Commit 43
    43  Skip
    44  TokenOpen \"]\"
    45  Char ']'
    46  TokenClose hidden
    47  CloseNode value
    48  Ret
rule STRING:
    49  Char '\\''
    50  Choice 53
    51  Class [^']
    52  LoopCommit 51
    53  Char '\\''
    54  Ret
skip:
    55  QuietOn
    56  Choice 61
    57  RepOpen {1,} 60
    58  Char ' '
    59  RepNext 58
    60  LoopCommit 57
    61  QuietOff
    62  Ret
entry STRING:
    63  Skip
    64  TokenOpen STRING
    65  Call STRING
    66  TokenClose
    67  Skip
    68  End
entry pair:
    69  Call pair
    70  Skip
    71  End
entry start:
    72  Call start
    73  Skip
    74  End
entry value:
    75  Call value
    76  Skip
    77  End
shared 0:
    78  Call value
    79  Ret
";
    assert_eq!(program.to_string(), expected);
}

#[test]
fn repetitions_lookahead_labels_and_flags_compile_to_their_templates() {
    let program = parser(concat!(
        "start: \"a\"? \"b\"* \"c\"+ \"d\"{2,3} &\"e\" !\"f\" (\"g\" ^ \"need g\") ",
        "%on(x, \"h\") %if(!x) \"ij\"i\n",
        "%flag x\n",
    ))
    .program;
    let expected = "\
rule start:
     0  Open
     1  Choice 6
     2  TokenOpen \"a\"
     3  Char 'a'
     4  TokenClose hidden
     5  Commit 6
     6  Choice 11
     7  TokenOpen \"b\"
     8  Char 'b'
     9  TokenClose hidden
    10  LoopCommit 7
    11  RepOpen {1,} 16
    12  TokenOpen \"c\"
    13  Char 'c'
    14  TokenClose hidden
    15  RepNext 12
    16  RepOpen {2,3} 21
    17  TokenOpen \"d\"
    18  Char 'd'
    19  TokenClose hidden
    20  RepNext 17
    21  PredChoice 26
    22  TokenOpen \"e\"
    23  Char 'e'
    24  TokenClose hidden
    25  BackCommit 27
    26  Fail
    27  PredChoice 32
    28  TokenOpen \"f\"
    29  Char 'f'
    30  TokenClose hidden
    31  FailTwice
    32  LabelChoice 37
    33  TokenOpen \"g\"
    34  Char 'g'
    35  TokenClose hidden
    36  Commit 38
    37  Throw \"need g\"
    38  FlagOn 0
    39  TokenOpen \"h\"
    40  Char 'h'
    41  TokenClose hidden
    42  FlagsPop
    43  FlagTest !0
    44  TokenOpen \"ij\"i
    45  StrFold \"ij\"
    46  TokenClose hidden
    47  CloseNode start
    48  Ret
entry start:
    49  Call start
    50  End
";
    assert_eq!(program.to_string(), expected);
}

#[test]
fn a_class_matches_exactly_the_code_points_its_set_contains() {
    let property = |property, negated| ClassItem::Property { property, negated };
    let sets = [
        (
            false,
            vec![ClassItem::Range('a', 'z'), ClassItem::Range('_', '_')],
        ),
        (
            true,
            vec![ClassItem::Range('"', '"'), ClassItem::Range('\\', '\\')],
        ),
        (false, vec![property(Property::XidStart, false)]),
        (true, vec![property(Property::Lowercase, false)]),
        (false, vec![property(Property::WhiteSpace, true)]),
        (false, vec![ClassItem::Range('é', 'ü')]),
        (true, vec![ClassItem::Range('\u{0}', '\u{7f}')]),
        (false, vec![ClassItem::Range('\u{0}', '\u{10ffff}')]),
        (false, vec![ClassItem::Range('x', 'Ω')]),
    ];
    let beyond = [
        'é',
        'ü',
        'ß',
        'Ω',
        'Ä',
        '中',
        '😀',
        '\u{80}',
        '\u{a0}',
        '\u{10ffff}',
    ];
    let chars = (0..0x80u8).map(char::from).chain(beyond);
    for c in chars {
        let text = format!("{c}rest");
        for (negated, items) in &sets {
            let set = CharSet {
                negated: *negated,
                items: items.clone(),
            };
            let expected = set.contains(c).then_some(c.len_utf8());
            assert_eq!(
                Class::new(&set).matches(&text, &mut 0),
                expected,
                "{c:?} in {set:?}"
            );
        }
    }
}

/// The first position of an instruction the predicate picks, in the routine of `start`.
fn find(program: &Program, pick: impl Fn(&Inst) -> bool) -> usize {
    program
        .code
        .iter()
        .position(pick)
        .expect("the program has it")
}

/// A broken program: what the verifier must say about it, and how to break it.
type Break = (&'static str, fn(&mut Program));

/// A grammar whose program has a choice, a commit, a call, a remembered call, a token and a
/// node.
const VERIFIED: &str = "start: x=\"a\" (\"b\" | \"c\") m\n@memo\nm: \"d\"\n";

#[test]
fn the_verifier_rejects_programs_that_break_the_machine_s_rules() {
    assert_eq!(verify(&parser(VERIFIED).program), Ok(()));
    fn choice(p: &Program) -> usize {
        find(p, |i| matches!(i, Inst::Choice(_)))
    }
    let breaks: [Break; 8] = [
        ("no rule 1000", |p| {
            let call = find(p, |i| matches!(i, Inst::MemoCall(_)));
            p.code[call] = Inst::MemoCall(1_000);
        }),
        ("remembers a rule that is not remembered", |p| {
            p.rules.iter_mut().for_each(|rule| rule.memo = false);
        }),
        ("expects [Choice, Label] on top", |p| {
            let at = choice(p);
            p.code[at] = Inst::Any;
        }),
        ("leaves the program", |p| {
            let at = choice(p);
            p.code[at] = Inst::Choice(1_000);
        }),
        ("leaves Shape", |p| {
            let close = find(p, |i| matches!(i, Inst::CloseNode(_)));
            p.code[close] = Inst::Any;
        }),
        ("calls no rule", |p| {
            let call = find(p, |i| matches!(i, Inst::Call(_)));
            p.code[call] = Inst::Call(1);
        }),
        ("opens a token inside a token", |p| {
            let close = find(p, |i| matches!(i, Inst::TokenClose { .. }));
            p.code[close] = p.code[close - 2];
        }),
        ("two shapes of the stack", |p| {
            let after_choice = choice(p) as u32 + 1;
            let commit = find(p, |i| matches!(i, Inst::Commit(_)));
            p.code[commit] = Inst::Commit(after_choice);
        }),
    ];
    for (problem, broken) in breaks {
        let mut program = parser(VERIFIED).program;
        broken(&mut program);
        let verdict = verify(&program);
        assert!(
            verdict.as_ref().is_err_and(|e| e.contains(problem)),
            "{problem}: {verdict:?}\n{program}"
        );
    }
}

#[test]
fn a_cut_without_an_entry_of_its_own_choice_gets_a_guard() {
    // The last alternative of the group has no entry, so its cut would mark the choice of the
    // rule. The last alternative of the rule has none around it and needs no guard.
    let program = parser("start: (\"a\" | \"b\" ~ \"c\") \"d\" | \"e\" ~ \"f\"\n").program;
    let expected = "\
rule start:
     0  Choice 22
     1  Open
     2  Choice 7
     3  TokenOpen \"a\"
     4  Char 'a'
     5  TokenClose hidden
     6  Commit 17
     7  Choice 16
     8  TokenOpen \"b\"
     9  Char 'b'
    10  TokenClose hidden
    11  Cut
    12  TokenOpen \"c\"
    13  Char 'c'
    14  TokenClose hidden
    15  Commit 17
    16  Fail
    17  TokenOpen \"d\"
    18  Char 'd'
    19  TokenClose hidden
    20  CloseNode start
    21  Commit 31
    22  Open
    23  TokenOpen \"e\"
    24  Char 'e'
    25  TokenClose hidden
    26  Cut
    27  TokenOpen \"f\"
    28  Char 'f'
    29  TokenClose hidden
    30  CloseNode start
    31  Ret
entry start:
    32  Call start
    33  End
";
    assert_eq!(program.to_string(), expected);
}

#[test]
fn the_options_decide_which_calls_remember() {
    let grammar = Grammar::from_text("start: m m\n@memo\nm: \"a\"\n").expect("it reads");
    let remembered = |memo| {
        let parser = Parser::with_options(&grammar, &Options { memo }).expect("it compiles");
        let calls = parser.program.code.iter();
        calls.filter(|i| matches!(i, Inst::MemoCall(_))).count()
    };
    // The two calls in `start` and the one that starts a parse from `m`, then none, then also
    // the call that starts a parse from `start`.
    assert_eq!(
        [Memo::Grammar, Memo::Off, Memo::All].map(remembered),
        [3, 0, 4]
    );
}

#[test]
fn the_levels_of_a_precedence_block_are_remembered_unless_it_says_no() {
    let text = |mark: &str| {
        format!(
            "start: expr\n{mark}expr: %precedence {{\n    neg: \"-\" neg -> neg\n    atom: \"1\"\n}}\n"
        )
    };
    let remembered = |mark: &str| {
        let code = parser(&text(mark)).program.code;
        let calls = code.iter().filter(|i| matches!(i, Inst::MemoCall(_)));
        calls.count()
    };
    // The calls of `neg` from the block and from itself, the call of `atom` that `neg` falls
    // through to, and those that start a parse from either level.
    assert_eq!(["", "@nomemo\n"].map(remembered), [5, 0]);
}

#[test]
fn a_remembered_rule_is_matched_once_at_a_position() {
    // Each alternative of `start` matches `item` afresh, so without the mark thirty levels of
    // brackets take 3^30 matches of the innermost one.
    let text = |mark: &str| {
        format!("start: item \".\" | item \"!\" | item\n{mark}item: \"(\" start \")\" | \"x\"\n")
    };
    let input = format!("{}x{}", "(".repeat(30), ")".repeat(30));
    let limits = Limits {
        max_steps: 100_000,
        ..Limits::default()
    };
    let marked = parser(&text("@memo\n")).parse_with(&input, "start", limits);
    assert!(marked.is_ok(), "{marked:?}");
    let plain = parser(&text("")).parse_with(&input, "start", limits);
    assert_eq!(
        plain.err().map(|error| error.kind),
        Some(ParseErrorKind::Limit(Limit::Steps))
    );
}

#[test]
fn left_recursion_columns_and_captures_wait_for_their_milestones() {
    let cases = [
        ("start: start \"a\" | \"a\"\n", "start is left recursive"),
        (
            "start: a\na: b \"x\"\nb: \"y\"? start\n",
            "a is left recursive",
        ),
        ("start: &start \"a\"\n", "start is left recursive"),
        (
            "start: e\ne: %precedence {\n    sum: sum \"+\" atom\n    atom: \"1\"\n}\n",
            "sum is left recursive",
        ),
        ("start: %indent(\"a\")\n", "start uses %indent or %same"),
        ("start: %def(x, \"a\") %ref(x)\n", "start uses %def or %ref"),
        (
            "start: \"a\"\n%ignore %def(x, \" \")\n",
            "the %ignore expression uses %def or %ref",
        ),
    ];
    for (text, problem) in cases {
        let errors = errors(text);
        assert!(
            errors.iter().any(|e| e.contains(problem)),
            "{text}: {errors:?}"
        );
    }
    // Recursion after input, and before a lookahead that consumes nothing, stays allowed.
    for text in [
        "start: \"a\" start | \"b\"\n",
        "start: !\"a\" \"b\" start?\n",
    ] {
        assert_eq!(errors(text), Vec::<String>::new(), "{text}");
    }
}

#[test]
fn nested_separated_repetitions_compile_to_code_linear_in_their_depth() {
    let mut expr = lit("a");
    for _ in 0..40 {
        expr = sep_many(expr, lit(","));
    }
    let mut g = GrammarBuilder::new();
    g.rule("start", r("_items"));
    g.rule("_items", expr);
    let parser = Parser::new(&g.build()).expect("the grammar compiles");
    // Forty levels that each copied their item would make more than a trillion instructions.
    assert!(parser.program.code.len() < 2_000, "{parser:?}");
    let input = "a,a";
    assert!(parser.parse(input, "start").is_ok());
}

#[test]
fn a_long_sequence_of_parts_that_may_be_empty_compiles_in_linear_time() {
    // The search for left recursion comes back to the sequence once per part, and finding its
    // left children afresh each time made the search quadratic in the number of parts.
    let parts: Vec<Expr> = (0..100_000).map(|_| opt(lit("a"))).collect();
    let mut g = GrammarBuilder::new();
    g.rule("start", Expr::new(ExprKind::Sequence(parts)));
    let started = std::time::Instant::now();
    assert!(Parser::new(&g.build()).is_ok());
    let seconds = started.elapsed().as_secs_f64();
    assert!(seconds < LINEAR_COMPILE_SECONDS, "{seconds:.1} s");
}

/// Time the long sequence may take to compile in a debug build. Linear, it takes well under a
/// second, quadratic about half a minute.
const LINEAR_COMPILE_SECONDS: f64 = 10.0;

#[test]
fn the_deepest_grammars_compile_on_a_small_stack() {
    // The resolver and the compiler recurse once per level of an expression, which the resolver
    // bounds: 100 levels in a rule, and twice that in a template instance whose argument nests
    // as deeply as its body.
    let mut nested = lit("a");
    for _ in 1..MAX_EXPRESSION_DEPTH {
        nested = sep_many(nested, lit(","));
    }
    let mut g = GrammarBuilder::new();
    g.rule("start", nested);
    let deep = |mut expr: Expr| {
        for _ in 2..MAX_EXPRESSION_DEPTH {
            expr = opt(expr);
        }
        expr
    };
    let mut h = GrammarBuilder::new();
    h.rule("start", r_args("t", [deep(lit("a"))]));
    h.rule("t", seq([deep(r("x")), lit("b")])).params(["x"]);
    for grammar in [g.build(), h.build()] {
        let compiled = std::thread::Builder::new()
            .stack_size(SMALL_STACK_BYTES)
            .spawn(move || Parser::new(&grammar).is_ok())
            .expect("the thread starts")
            .join()
            .expect("compiling finishes");
        assert!(compiled);
    }
}

/// A stack of the size Windows gives a program's main thread.
const SMALL_STACK_BYTES: usize = 1024 * 1024;
