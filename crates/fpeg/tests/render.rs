// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Errors shown with the line they stand in, as the command line and the Python package show
//! them.

use std::time::Instant;

use fpeg::{Diagnostic, DiagnosticCode, Grammar, LineIndex, ParseError, Parser, Source, Span};

/// A list whose items and closing bracket are named errors, with or without the recovery of
/// the bracket.
fn list(close_recovers: bool) -> Parser {
    let mut text = String::from(
        "start: \"[\" (item (\",\" (item ^ item))*)? \"]\" ^ close\n\
         item: NUMBER\nNUMBER: [0-9]+\n%ignore \" \"+\n%recover item: (!(\",\" | \"]\") .)*\n",
    );
    if close_recovers {
        text.push_str("%recover close: ()\n");
    }
    let grammar = Grammar::from_text(&text).expect("the grammar reads");
    Parser::new(&grammar).expect("the grammar compiles")
}

fn failure(parser: &Parser, input: &str) -> ParseError {
    parser
        .parse_recovering(input, "start")
        .expect_err("the input has an error that does not recover")
}

#[test]
fn errors_show_their_name_place_and_line_with_carets_under_the_span() {
    let input = "[1, xy, 3";
    let source = Source::new(input, Some("data.txt"));
    let tree = list(true)
        .parse_recovering(input, "start")
        .expect("the errors recover");
    let shown: Vec<String> = (tree.recoveries().iter())
        .map(|r| r.render(&source))
        .collect();
    assert_eq!(
        shown.join("\n\n"),
        "error[item]: expected NUMBER\n \
         --> data.txt:1:5\n  |\n1 | [1, xy, 3\n  |     ^^\n\n\
         error[close]: expected \",\", \"]\"\n \
         --> data.txt:1:10\n  |\n1 | [1, xy, 3\n  |          ^"
    );
    // A text whose lines are indexed already shows the same.
    let lines = LineIndex::new(input);
    let indexed = Source::indexed(input, Some("data.txt"), &lines);
    assert_eq!(tree.recoveries()[0].render(&indexed), shown[0]);
    // A parse that fails shows the recoveries on its way first.
    let error = failure(&list(false), input);
    let blocks = error.render(&source);
    assert!(blocks.starts_with("error[item]: expected NUMBER\n"));
    assert!(blocks.ends_with("\n\nerror[close]: expected \",\", \"]\"\n --> data.txt:1:10\n  |\n1 | [1, xy, 3\n  |          ^"));
}

#[test]
fn an_error_without_a_name_shows_its_message_and_one_without_a_place_its_title() {
    let grammar = Grammar::from_text("start: \"a\" (\"b\" ^ \"a b must follow\")\n")
        .expect("the grammar reads");
    let parser = Parser::new(&grammar).expect("the grammar compiles");
    let source = Source::new("ac", None);
    let error = parser.parse("ac", "start").expect_err("c is no b");
    assert_eq!(
        error.render(&source),
        "error: a b must follow\n --> 1:2\n  |\n1 | ac\n  |  ^"
    );
    let error = parser.parse("", "start").expect_err("the input is empty");
    assert_eq!(
        error.render(&source),
        "error: expected \"a\"\n --> 1:1\n  |\n1 | ac\n  | ^"
    );
    let error = parser
        .parse("ab", "nope")
        .expect_err("there is no rule nope");
    assert_eq!(error.render(&source), "error: no rule nope to start from");
}

#[test]
fn columns_follow_tabs_and_wide_characters_and_controls_show_as_marks() {
    // A tab stop every four columns, a combining accent, a wide character, ESC and a
    // right-to-left override.
    let text = "\te\u{301}\u{4e2d}\u{1b}\u{202e}\tx = 1";
    let source = Source::new(text, None);
    let x = text.find('x').expect("x stands in the text");
    assert_eq!(
        source.render(None, "oops", x, x),
        "error: oops\n --> 1:8\n  |\n\
         1 |     e\u{301}\u{4e2d}\u{241b}\u{fffd}   x = 1\n  |             ^"
    );
    // The message and the path show the same way, so neither can move the cursor.
    let source = Source::new(text, Some("a\u{1b}[2J.txt"));
    let shown = source.render(Some("n"), "n: bad\nline", x, x);
    assert!(shown.starts_with("error[n]: bad\u{240a}line\n --> a\u{241b}[2J.txt:1:8\n"));
}

#[test]
fn an_error_or_diagnostic_in_one_line_shows_controls_as_marks() {
    let grammar = Grammar::from_text("start: \"a\" (\"b\" ^ \"bad\\x1b[2J\")\n").expect("it reads");
    let parser = Parser::new(&grammar).expect("the grammar compiles");
    let error = parser.parse("ac", "start").expect_err("c is no b");
    assert_eq!(error.message(), "bad\u{1b}[2J");
    assert_eq!(error.to_string(), "error at 1: bad\u{241b}[2J");
    let diagnostic = Diagnostic {
        code: DiagnosticCode::Syntax,
        message: "x\u{202e}y".to_owned(),
        span: Span::default(),
    };
    assert_eq!(diagnostic.to_string(), "error[E101]: x\u{fffd}y at 0..0");
}

#[test]
fn a_long_line_is_cut_around_the_error() {
    let line: String = (0..300u32)
        .map(|i| char::from_digit(i % 10, 10).unwrap_or('?'))
        .collect();
    let source = Source::new(&line, None);
    assert_eq!(
        source.render(None, "oops", 200, 203),
        format!(
            "error: oops\n --> 1:201\n  |\n1 | ...{}...\n  | {}^^^",
            &line[160..277],
            " ".repeat(43)
        )
    );
    // Near its start a line is cut on its right only.
    assert_eq!(
        source.render(None, "oops", 10, 10),
        format!(
            "error: oops\n --> 1:11\n  |\n1 | {}...\n  | {}^",
            &line[..120],
            " ".repeat(10)
        )
    );
    // Combining marks take no column but count one each, so that a run of them is cut too.
    let marks = format!("x{}", "\u{301}".repeat(1_000));
    let source = Source::new(&marks, None);
    assert_eq!(
        source.render(None, "oops", 0, 1),
        format!(
            "error: oops\n --> 1:1\n  |\n1 | {}...\n  | ^",
            &marks[..239]
        )
    );
}

/// A line of code points beyond ASCII, too long to walk over for each error shown.
const LONG_LINE_CHARS: usize = 2_000_000;

/// Errors shown near the end of that line, one at a time.
const ERRORS_SHOWN: usize = 1_000;

/// Time that showing them stays well within where each costs what it shows, also in a build
/// without optimizations, where a walk over the line in fpeg's own code for each takes half a
/// minute.
const SHOW_SECONDS: f64 = 2.0;

#[test]
fn showing_an_error_costs_what_it_shows_however_long_its_line() {
    let text = "\u{e9}".repeat(LONG_LINE_CHARS);
    let source = Source::new(&text, None);
    let started = Instant::now();
    for k in 1..=ERRORS_SHOWN {
        let at = source.byte_offset(LONG_LINE_CHARS - k);
        assert_eq!(at, text.len() - 2 * k);
        let shown = source.render(None, "oops", at, at);
        assert!(shown.contains(&format!(" --> 1:{}\n", LONG_LINE_CHARS - k + 1)));
    }
    assert!(started.elapsed().as_secs_f64() < SHOW_SECONDS);
}

#[test]
fn a_span_over_lines_is_marked_to_the_end_of_its_first() {
    let text = format!("{}bcd\r\nefg", "a\r\n".repeat(9));
    let source = Source::new(&text, None);
    let c = text.find('c').expect("c stands in the text");
    assert_eq!(
        source.render(None, "oops", c, text.len()),
        "error: oops\n  --> 10:2\n   |\n10 | bcd\n   |  ^^"
    );
}
