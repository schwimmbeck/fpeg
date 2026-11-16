// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the compiler, the verifier and the machine that need their insides. What a parse
//! returns is compared with the reference interpreter in the crate's differential tests.

use std::collections::{HashMap, HashSet};

use super::machine::{Machine, Memory, Stop, memo_hashes};
use super::program::{ByteTest, Class, Inst, Program};
use super::verify::verify;
use super::{Memo, Options, Parser, Passes};
use crate::analysis::Bytes;
use crate::builder::{GrammarBuilder, lit, opt, r, r_args, sep_many, seq};
use crate::grammar::{ClassItem, Expr, ExprKind, Grammar, MAX_EXPRESSION_DEPTH, Property};
use crate::ir::CharSet;
use crate::parse::{Limit, Limits, ParseErrorKind};

fn parser(text: &str) -> Parser {
    let grammar = Grammar::from_text(text).unwrap_or_else(|e| panic!("{e:?}"));
    Parser::new(&grammar).unwrap_or_else(|e| panic!("{e:?}"))
}

/// A parser with these passes of the optimizer.
fn parser_with(text: &str, passes: Passes) -> Parser {
    let grammar = Grammar::from_text(text).unwrap_or_else(|e| panic!("{e:?}"));
    let options = Options {
        passes,
        ..Options::default()
    };
    Parser::with_options(&grammar, &options).unwrap_or_else(|e| panic!("{e:?}"))
}

/// A parser without the passes of the optimizer, whose program shows the plain templates.
fn plain(text: &str) -> Parser {
    parser_with(text, Passes::NONE)
}

/// Every pass but inlining, so that the routines of the rules show in their callers' code.
const CALLS_STAY: Passes = Passes {
    inline: false,
    ..Passes::ALL
};

/// The disassembly of the routine of one rule.
fn routine(program: &Program, rule: &str) -> String {
    let text = program.to_string();
    let start = text.find(&format!("rule {rule}:\n")).expect("the rule");
    let lines = text[start..].lines().skip(1);
    let lines = lines.take_while(|line| line.starts_with(' '));
    let body: Vec<&str> = lines.map(str::trim_start).collect();
    body.join("\n")
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
    let mut machine = Machine::new(&parser.program, &input, Limits::default(), &[]);
    assert_eq!(machine.run(parser.program.entries["start"]), Ok(()));
    let (tree, _) = machine.tree().expect("the tree fits");
    // Keeping them would hold the nodes of every one of the 2^12 failed attempts.
    assert!(
        machine.arena_len() <= tree.len(),
        "{} nodes in the arena for a tree of {}",
        machine.arena_len(),
        tree.len()
    );
}

#[test]
fn a_failure_cuts_the_arena_back_as_far_as_the_newest_remembered_node() {
    // As above, with a remembered rule matched after every closing bracket. Each of its first
    // matches keeps the arena as long as it is then, since the memo points at its node, and
    // there is one for each position. Here that leaves few of the failed attempts. A grammar
    // can place the remembered nodes so that most of them stay, bounded by the steps alone.
    let parser =
        parser("start: s\ns: \"(\" s \")\" t \"x\" | \"(\" s \")\" t | \"a\"\n@memo\nt: ()\n");
    let input = format!("{}a{}", "(".repeat(12), ")".repeat(12));
    let mut machine = Machine::new(&parser.program, &input, Limits::default(), &[]);
    assert_eq!(machine.run(parser.program.entries["start"]), Ok(()));
    let (tree, _) = machine.tree().expect("the tree fits");
    // The 2^12 failed attempts make more than twenty thousand nodes.
    assert!(
        machine.arena_len() <= 4 * tree.len(),
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
    let program = plain(concat!(
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
    let program = plain(concat!(
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
fn repetitions_of_a_class_compile_to_one_span() {
    let text = concat!(
        "start: A B C\n",
        "A: [a-z]* \" \"+ [0-9]{3,} \"é\"*\n",
        "B: ([a-z]+)* ([a-z]+)+ ([a-z]*){2,}\n",
        "C: ([a-z]+){2,} [a-z]{2,4} \"ab\"*\n",
    );
    let program = parser_with(text, CALLS_STAY).program;
    // The first step of `([a-z]+){2,}` takes the whole run, so a second step never matches,
    // and the repetition stays.
    let expected = "\
12  Span [a-z] {0,}
13  Span [ ] {1,}
14  Span [0-9] {3,}
15  Span [é] {0,}
16  Ret
17  Span [a-z] {0,}
18  Span [a-z] {1,}
19  Span [a-z] {0,}
20  Ret
21  RepOpen {2,} 24
22  Span [a-z] {1,}
23  RepNext 22
24  RepOpen {2,4} 27
25  Class [a-z]
26  RepNext 25
27  Choice 30
28  Str \"ab\"
29  LoopCommit 28
30  Ret";
    let rules = ["A", "B", "C"].map(|rule| routine(&program, rule));
    assert_eq!(rules.join("\n"), expected);
    let plain = plain("start: A\nA: [a-z]*\n").program.to_string();
    assert!(!plain.contains("Span"), "{plain}");
}

#[test]
fn code_that_cannot_start_with_the_next_byte_is_jumped_past() {
    let text = concat!(
        "start: WORD\n",
        "WORD: (A | B | \"c\")* A? (B | (\"x\" ^ \"need x\") | A)\n",
        "A: \"a\" \"1\"\n",
        "B: \"b\" \"2\"\n",
    );
    let program = parser_with(text, CALLS_STAY).program;
    // The step of the loop pops its entry where it cannot start. No test stands before `"c"`,
    // which tests the byte at once, or before the label, which would throw where its
    // expression fails.
    let expected = "\
6  Choice 18
7  Test pop [a-c] 18
8  Test [a] 12
9  Choice 12
10  MemoCall A
11  Commit 17
12  Test [b] 16
13  Choice 16
14  MemoCall B
15  Commit 17
16  Char 'c'
17  LoopCommit 7
18  Test [a] 22
19  Choice 22
20  MemoCall A
21  Commit 22
22  Test [b] 26
23  Choice 26
24  MemoCall B
25  Commit 34
26  Choice 32
27  LabelChoice 30
28  Char 'x'
29  Commit 31
30  Throw \"need x\"
31  Commit 34
32  TestFail [a]
33  MemoCall A
34  Ret";
    assert_eq!(routine(&program, "WORD"), expected);
}

#[test]
fn small_rules_compile_into_their_callers() {
    let program = parser(concat!(
        "start: A B C D E\n",
        "A: \"a\" X\n",
        "X: \"x\"\n",
        "@memo\nB: \"b\"\n",
        "C: \"c\" C?\n",
        "D: \"d\" ~ \"e\"\n",
        "E: \"1\" \"2\" \"3\" \"4\" \"5\" \"6\" \"7\" \"8\" \"9\" \"10\" \"11\" \"12\" \"13\" \"14\" ",
        "\"15\" \"16\" \"17\" \"18\" \"19\" \"20\" \"21\" \"22\" \"23\" \"24\"\n",
    ))
    .program;
    // `A` with `X` in it. Calls stay where the rule is remembered, calls itself, holds a cut
    // that would reach past its call, or has more than 24 expressions.
    let expected = "\
0  Open
1  TokenOpen A
2  Char 'a'
3  Char 'x'
4  TokenClose
5  TokenOpen B
6  MemoCall B
7  TokenClose
8  TokenOpen C
9  Call C
10  TokenClose
11  TokenOpen D
12  Call D
13  TokenClose
14  TokenOpen E
15  Call E
16  TokenClose
17  CloseNode start
18  Ret";
    assert_eq!(routine(&program, "start"), expected);
}

#[test]
fn rules_that_call_each_other_where_nothing_is_tried_compile_into_their_callers() {
    // A repetition of no steps never tries its expression, so neither call is made.
    let program = parser("start: A\nA: B{0} \"x\"\nB: A{0} \"y\"\n").program;
    assert_eq!(
        routine(&program, "start"),
        "0  Open\n1  TokenOpen A\n2  Char 'x'\n3  TokenClose\n4  CloseNode start\n5  Ret"
    );
}

#[test]
fn direct_left_recursion_runs_as_a_loop_over_its_seed() {
    let text =
        "start: sum\nsum: sum \"+\" N -> plus\n    | sum \"-\" N -> minus\n    | N\nN: \"1\"\n";
    let only_loops = Passes {
        left_loop: true,
        ..Passes::NONE
    };
    let program = parser_with(text, only_loops).program;
    // The base, then a pass over the recursive alternatives, each with the seed where its call
    // of `sum` stood, for as long as one matches and ends further.
    let expected = "\
4  SeedOpen
5  Open
6  TokenOpen N
7  Call N
8  TokenClose
9  CloseNode sum
10  SeedKeep
11  Choice 33
12  Choice 23
13  Open
14  SeedUse
15  TokenOpen \"+\"
16  Char '+'
17  TokenClose hidden
18  TokenOpen N
19  Call N
20  TokenClose
21  CloseNode plus
22  Commit 32
23  Open
24  SeedUse
25  TokenOpen \"-\"
26  Char '-'
27  TokenClose hidden
28  TokenOpen N
29  Call N
30  TokenClose
31  CloseNode minus
32  SeedGrow 10
33  SeedClose
34  Ret";
    assert_eq!(routine(&program, "sum"), expected);
    assert!(!program.rules.iter().any(|rule| rule.recursive));
}

#[test]
fn a_loop_over_a_class_and_other_alternatives_takes_runs_of_the_class() {
    // `(CHAR | "\\" .)*` as `CHAR* ("\\" . CHAR*)*`, the class reached through the call of
    // `CHAR`, compiled as its body.
    let program = parser("start: S\nS: (CHAR | \"\\\\\" .)*\nCHAR: [a-z]\n").program;
    let expected = "\
11  Span [a-z] {0,}
12  Choice 17
13  Char '\\\\'
14  Any
15  Span [a-z] {0,}
16  LoopCommit 13
17  Ret";
    assert_eq!(routine(&program, "S"), expected);
}

#[test]
fn a_skip_expression_that_repeats_a_class_is_taken_as_one_run() {
    let text = "start: \"x\"\n%ignore [ \\t]+\n";
    let program = parser(text).program;
    assert_eq!(program.skip, None);
    let shown = program.to_string();
    assert!(
        shown.ends_with("skip:\n        Span [ \\t] {0,}\n"),
        "{shown}"
    );
    assert!(plain(text).program.skip.is_some());
}

#[test]
fn a_skip_from_where_the_last_one_started_with_its_flags_is_answered_at_once() {
    // Each alternative skips the same spaces before its token. The skip routine takes four
    // steps a space, so three runs of it do not fit the limit, one does.
    let text = "start: \"x\" | \"y\" | \"z\"\n%ignore \" \" | \"#\"\n";
    let input = format!("{}z", " ".repeat(1_000));
    let limits = Limits {
        max_steps: 8_000,
        ..Limits::default()
    };
    let answered = parser(text).parse_with(&input, "start", limits);
    assert!(answered.is_ok(), "{answered:?}");
    let plain = plain(text).parse_with(&input, "start", limits);
    assert_eq!(
        plain.err().map(|error| error.kind),
        Some(ParseErrorKind::Limit(Limit::Steps))
    );
}

#[test]
fn a_skip_at_a_byte_its_expression_cannot_start_with_ends_at_once() {
    // A skip before each "x" finds nothing to skip. Without the skip pass the routine runs for
    // each and takes five steps more, which do not fit the limit.
    let text = "start: \"x\"*\n%ignore \" \" | \"#\"\n";
    let input = "x".repeat(1_000);
    let limits = Limits {
        max_steps: 9_000,
        ..Limits::default()
    };
    let answered = parser(text).parse_with(&input, "start", limits);
    assert!(answered.is_ok(), "{answered:?}");
    let routine = parser_with(
        text,
        Passes {
            skip: false,
            ..Passes::ALL
        },
    );
    assert_eq!(
        (routine.parse_with(&input, "start", limits).err()).map(|error| error.kind),
        Some(ParseErrorKind::Limit(Limit::Steps))
    );
    // Where the routine runs, its loop ends at such a byte at once too.
    let shown = parser(text).program.to_string();
    let skip = shown.split("skip:").nth(1).unwrap_or_default();
    assert!(skip.contains("Test pop"), "{shown}");
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

/// The first position of an instruction the predicate picks, anywhere in the program.
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
    let breaks: [Break; 20] = [
        ("no rule 1000", |p| {
            let call = find(p, |i| matches!(i, Inst::MemoCall(_)));
            p.code[call] = Inst::MemoCall(1_000);
        }),
        ("no class 1000", |p| {
            let at = find(p, |i| matches!(i, Inst::Char(_)));
            p.code[at] = Inst::Span {
                class: 1_000,
                min: 0,
            };
        }),
        ("remembers a rule that is not remembered", |p| {
            p.rules.iter_mut().for_each(|rule| rule.memo = false);
        }),
        ("remembers a rule that is left recursive", |p| {
            p.rules.iter_mut().for_each(|rule| rule.recursive = true);
        }),
        ("calls a rule that needs a frame", |p| {
            let call = find(p, |i| matches!(i, Inst::MemoCall(_)));
            let Inst::MemoCall(rule) = p.code[call] else {
                return;
            };
            p.code[call] = Inst::Call(p.rules[rule as usize].pc);
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
        ("no test 1000", |p| {
            let at = choice(p);
            p.code[at] = Inst::TestFail(1_000);
        }),
        ("expects [Choice] on top", |p| {
            let at = choice(p);
            p.tests.push(ByteTest {
                bytes: Bytes::ALL,
                after_skip: false,
                pop: true,
                kinds: Box::default(),
            });
            let test = (p.tests.len() - 1) as u32;
            p.code[at] = Inst::Test {
                test,
                target: at as u32 + 1,
            };
        }),
        ("pops before it fails", |p| {
            let at = choice(p);
            p.tests.push(ByteTest {
                bytes: Bytes::ALL,
                after_skip: false,
                pop: true,
                kinds: Box::default(),
            });
            p.code[at] = Inst::TestFail((p.tests.len() - 1) as u32);
        }),
        ("uses a seed outside a loop", |p| {
            let at = choice(p);
            p.code[at] = Inst::SeedUse;
        }),
        ("expects [Loop] on top", |p| {
            let at = choice(p);
            p.code[at] = Inst::SeedClose;
        }),
        ("expects [Mark] on top", |p| {
            let at = choice(p);
            p.code[at] = Inst::Same { skip: true };
        }),
        ("expects [Column] on top", |p| {
            let at = choice(p);
            p.code[at] = Inst::IndentEnd;
        }),
        ("expects [Def] on top", |p| {
            let at = choice(p);
            p.code[at] = Inst::DefClose(0);
        }),
        ("expects [Captures] on top", |p| {
            let at = choice(p);
            p.code[at] = Inst::CaptureKeep;
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
    let program = plain("start: (\"a\" | \"b\" ~ \"c\") \"d\" | \"e\" ~ \"f\"\n").program;
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
        let options = Options {
            memo,
            ..Options::default()
        };
        let parser = Parser::with_options(&grammar, &options).expect("it compiles");
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
fn the_expression_grammar_remembers_the_levels_tried_twice_and_no_others() {
    let text = include_str!("../../grammars/expressions.fpeg");
    let remembered = |text: &str| {
        let rules = parser(text).program.rules;
        let names = rules.iter().filter(|rule| rule.memo);
        names.map(|rule| rule.name.clone()).collect::<Vec<_>>()
    };
    // Both alternatives of `conditional` start with `or`, and so do both of the level above
    // with `conditional`. Every other level runs as a loop or is tried once at a position.
    assert_eq!(remembered(text), ["conditional", "or"]);
    let unmarked = text.replace("expr: %precedence", "@nomemo\nexpr: %precedence");
    assert_eq!(remembered(&unmarked), Vec::<String>::new());
}

#[test]
fn a_rule_that_a_choice_tries_twice_is_matched_once_at_a_position() {
    // Each alternative of `start` starts with `item`, which is remembered without a mark, and
    // without it thirty levels of brackets take 3^30 matches of the innermost one.
    let text = |mark: &str| {
        format!("start: item \".\" | item \"!\" | item\n{mark}item: \"(\" start \")\" | \"x\"\n")
    };
    let input = format!("{}x{}", "(".repeat(30), ")".repeat(30));
    let limits = Limits {
        max_steps: 100_000,
        ..Limits::default()
    };
    let remembered = parser(&text("")).parse_with(&input, "start", limits);
    assert!(remembered.is_ok(), "{remembered:?}");
    let unmarked = parser(&text("@nomemo\n")).parse_with(&input, "start", limits);
    assert_eq!(
        unmarked.err().map(|error| error.kind),
        Some(ParseErrorKind::Limit(Limit::Steps))
    );
}

#[test]
fn the_memo_spreads_positions_at_any_power_of_two_apart_over_its_buckets() {
    // 100,000 random keys take about 95,400 of the values of 20 low bits.
    for shift in 0..13 {
        let hashes = memo_hashes((0..100_000u32).map(|i| (7, i << shift, 0)));
        let low: HashSet<u64> = hashes.iter().map(|hash| hash & 0xf_ffff).collect();
        assert!(low.len() > 90_000, "{} apart: {}", 1 << shift, low.len());
    }
}

#[test]
fn the_memo_of_each_parse_hashes_keys_with_keys_of_its_own() {
    // Keys known before a parse would let its grammar and input choose calls that collide.
    let calls = || (0..64u32).map(|i| (3, i, u64::from(i) << 40));
    assert_ne!(memo_hashes(calls()), memo_hashes(calls()));
}

#[test]
fn room_that_grows_counts_the_old_room_until_it_is_given_back() {
    // Sixteen words, then thirty-two, which move, so that both blocks are held for a moment.
    let grown = |limit| {
        let mut memory = Memory::new(limit);
        let mut words: Vec<u64> = Vec::new();
        for word in 0..17 {
            memory.room(&mut words, 1)?;
            words.push(word);
        }
        Ok::<u64, Stop>(memory.held())
    };
    assert!(grown(16 * 8 + 32 * 8 - 1).is_err());
    assert_eq!(grown(16 * 8 + 32 * 8).ok(), Some(32 * 8));
    // A table moves from four buckets to eight, each of an entry and a byte of control.
    let bucket = 2 * 8 + 1;
    let filled = |limit| {
        let mut memory = Memory::new(limit);
        let mut table: HashMap<u64, u64> = HashMap::new();
        for key in 0..4 {
            memory.room_in(&mut table)?;
            table.insert(key, key);
        }
        Ok::<u64, Stop>(memory.held())
    };
    assert!(filled((4 + 8) * bucket - 1).is_err());
    assert_eq!(filled((4 + 8) * bucket).ok(), Some(8 * bucket));
}

#[test]
fn the_rules_on_left_cycles_and_no_others_take_part_in_left_recursion() {
    let cases = [
        ("start: start \"a\" | \"a\"\n", vec!["start"]),
        (
            "start: a\na: b \"x\"\nb: \"y\"? start\n",
            vec!["start", "a", "b"],
        ),
        ("start: &start \"a\"\n", vec!["start"]),
        (
            "start: e\ne: %precedence {\n    sum: sum \"+\" atom\n    atom: \"1\"\n}\n",
            vec!["sum"],
        ),
        // Recursion after input, and after a lookahead that consumes nothing, is not left.
        ("start: \"a\" start | \"b\"\n", vec![]),
        ("start: !\"a\" \"b\" start?\n", vec![]),
    ];
    for (text, expected) in cases {
        let rules = plain(text).program.rules;
        let recursive = rules.iter().filter(|rule| rule.recursive);
        let names: Vec<&str> = recursive.map(|rule| rule.name.as_str()).collect();
        assert_eq!(names, expected, "{text}");
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
fn a_rule_compiled_into_its_callers_adds_its_classes_to_the_program_once() {
    // Copied at every call, a class of many code points would take memory with each.
    let calls = vec!["word"; 100].join(" ");
    let parser = parser(&format!("start: {calls}\nword: [a-z] [a-z0-9]*\n"));
    assert!(parser.program.code.len() > 200, "{parser:?}");
    assert_eq!(parser.program.classes.len(), 2);
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
fn the_rules_a_choice_point_can_call_twice_are_found_in_linear_time() {
    // Gathering the rules that a part or alternative calls afresh after each one took time
    // quadratic in their number.
    let calls = || (0..TWICE_CALLS).map(|i| r(&format!("r{i}")));
    let sequence = Expr::new(ExprKind::Sequence(calls().map(opt).collect()));
    let choice = Expr::new(ExprKind::Choice(calls().collect()));
    for start in [sequence, choice] {
        let mut g = GrammarBuilder::new();
        g.rule("start", start);
        for i in 0..TWICE_CALLS {
            g.rule(&format!("r{i}"), lit("a"));
        }
        let grammar = crate::resolve::resolve(&g.build()).expect("it resolves");
        let facts = crate::analysis::analyze(&grammar);
        let started = std::time::Instant::now();
        let twice = facts.tried_twice(&grammar);
        let seconds = started.elapsed().as_secs_f64();
        assert!(seconds < TWICE_SECONDS, "{seconds:.1} s");
        assert!(!twice.contains(&true));
    }
}

/// The parts of the sequence and the alternatives of the choice above, each a call of a rule of
/// its own.
const TWICE_CALLS: usize = 50_000;

/// Time the search may take in a debug build. Linear, it takes a few hundredths of a second,
/// quadratic several seconds.
const TWICE_SECONDS: f64 = 1.0;

#[test]
fn the_starts_of_a_chain_of_rules_are_found_with_each_rule_after_those_it_calls() {
    // Each rule is defined after the one it calls. Taken before them, a rule passed on every
    // kind of token that the choice at the bottom added, up the whole chain again.
    let mut g = GrammarBuilder::new();
    for i in 0..CHAIN_KINDS {
        g.rule(&format!("K{i}"), lit(&format!("k{i}")));
    }
    let tokens = (0..CHAIN_KINDS).map(|i| r(&format!("K{i}")));
    g.rule("c0", Expr::new(ExprKind::Choice(tokens.collect())));
    for i in 1..=CHAIN_RULES {
        g.rule(&format!("c{i}"), r(&format!("c{}", i - 1)));
    }
    g.rule("start", r(&format!("c{CHAIN_RULES}")));
    let grammar = crate::resolve::resolve(&g.build()).expect("it resolves");
    let facts = crate::analysis::analyze(&grammar);
    let started = std::time::Instant::now();
    let starts = facts.starts(&grammar, &vec![false; grammar.rules.len()]);
    let seconds = started.elapsed().as_secs_f64();
    assert!(seconds < CHAIN_SECONDS, "{seconds:.1} s");
    let start = grammar.rules.iter().find(|rule| rule.name == "start");
    let heads = start.and_then(|rule| starts.heads(&rule.body));
    assert_eq!(heads.map(<[_]>::len), Some(CHAIN_KINDS));
}

/// The rules of the chain above, and the kinds of tokens of the choice at its bottom, as many
/// as are kept.
const CHAIN_RULES: usize = 10_000;
const CHAIN_KINDS: usize = 64;

/// Time finding the starts may take in a debug build: about a tenth of a second children first,
/// several seconds parents first.
const CHAIN_SECONDS: f64 = 1.0;

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
