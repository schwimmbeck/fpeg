// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Runs the built `fpeg` binary.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// A grammar of sums, with a token, a hidden token, a node and a field.
const SUMS: &str = "start: first=N (\"+\" N)*\nN: [0-9]+\n%ignore \" \"+\n";

fn run<I: IntoIterator<Item = S>, S: AsRef<OsStr>>(args: I) -> Output {
    run_with_input(args, "")
}

fn run_with_input<I: IntoIterator<Item = S>, S: AsRef<OsStr>>(args: I, input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fpeg"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the fpeg binary runs");
    child
        .stdin
        .take()
        .expect("standard input is piped")
        .write_all(input.as_bytes())
        .expect("the input is written");
    child.wait_with_output().expect("the binary finishes")
}

/// Write files into a directory of the test's own and return their paths.
fn files(test: &str, contents: &[(&str, &str)]) -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(test);
    std::fs::create_dir_all(&dir).expect("the directory is made");
    (contents.iter())
        .map(|(name, text)| {
            let path = dir.join(name);
            std::fs::write(&path, text).expect("the file is written");
            path
        })
        .collect()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(unix)]
fn not_unicode() -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(vec![0xff])
}

#[cfg(windows)]
fn not_unicode() -> OsString {
    use std::os::windows::ffi::OsStringExt;
    // A lone surrogate is not valid UTF-16.
    OsString::from_wide(&[0xd800])
}

#[test]
fn version_flag_prints_the_core_version() {
    let out = run(["--version"]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("fpeg {}\n", fpeg::VERSION)
    );
}

#[test]
fn unaccepted_argument_is_a_usage_error_that_names_it() {
    for args in [
        &["--version", "--no-such-flag"][..],
        &["parse", "--no-such-flag"],
        &["check", "--no-such-flag"],
        &["profile", "--no-such-flag"],
    ] {
        let out = run(args);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("unexpected argument '--no-such-flag'"),
            "{stderr}"
        );
    }
}

#[test]
fn argument_that_is_not_unicode_is_a_usage_error() {
    let out = run([not_unicode()]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn parse_prints_the_tree_one_item_per_line() {
    let paths = files("lines", &[("sums.fpeg", SUMS), ("input.txt", "1 + 23")]);
    let out = run(["parse".as_ref(), paths[0].as_os_str(), paths[1].as_os_str()]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "start\n  first=N\"1\"\n  ~\"+\"\n  N\"23\"\n"
    );
    let out = run([
        "parse",
        "--spans",
        &paths[0].to_string_lossy(),
        &paths[1].to_string_lossy(),
    ]);
    assert!(text(&out.stdout).starts_with("start@0..6\n  first=N\"1\"@0..1\n"));
}

#[test]
fn parse_reads_standard_input_and_prints_json() {
    let paths = files("json", &[("sums.fpeg", SUMS)]);
    let out = run_with_input(
        ["parse".as_ref(), "--json".as_ref(), paths[0].as_os_str()],
        "1+2",
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        concat!(
            "[{\"kind\":\"start\",\"start\":0,\"end\":3,\"children\":[",
            "{\"kind\":\"N\",\"field\":\"first\",\"start\":0,\"end\":1,\"text\":\"1\"},",
            "{\"kind\":\"\\\"+\\\"\",\"start\":1,\"end\":2,\"text\":\"+\",\"hidden\":true},",
            "{\"kind\":\"N\",\"start\":2,\"end\":3,\"text\":\"2\"}]}]\n"
        )
    );
}

#[test]
fn json_escapes_the_characters_a_terminal_acts_on() {
    let paths = files("json_controls", &[("any.fpeg", "start: C*\nC: .\n")]);
    let out = run_with_input(
        ["parse".as_ref(), "--json".as_ref(), paths[0].as_os_str()],
        "\u{9b}\u{202e}\u{7f}",
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    for escaped in ["\\u009b", "\\u202e", "\\u007f"] {
        assert!(
            shown.contains(&format!("\"text\":\"{escaped}\"")),
            "{shown}"
        );
    }
}

#[test]
fn a_failed_parse_reports_where_and_what_was_expected() {
    let paths = files("failed", &[("sums.fpeg", SUMS)]);
    let out = run_with_input(["parse".as_ref(), paths[0].as_os_str()], "1 +\n+ 2");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    // Only spaces are ignored, so the line break is where an N was expected.
    assert_eq!(
        text(&out.stderr),
        "error: expected N\n --> <stdin>:1:4\n  |\n1 | 1 +\n  |    ^\n\n"
    );
}

#[test]
fn parse_with_recover_prints_the_recovered_tree_and_fails_with_each_error() {
    let list = "start: \"[\" (N (\",\" (N ^ item))*)? \"]\" ^ close\nN: [0-9]+\n\
                %ignore \" \"+\n%recover item: (!(\",\" | \"]\") .)*\n%recover close: ()\n";
    let paths = files("recover", &[("list.fpeg", list)]);
    let args = ["parse".as_ref(), "--recover".as_ref(), paths[0].as_os_str()];
    let out = run_with_input(args, "[1, x]");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        text(&out.stdout),
        "start\n  ~\"[\"\n  N\"1\"\n  ~\",\"\n  <error>\n    ~\"x\"\n  ~\"]\"\n"
    );
    assert_eq!(
        text(&out.stderr),
        "error[item]: expected N\n --> <stdin>:1:5\n  |\n1 | [1, x]\n  |     ^\n\n"
    );
    // A parse that fails even so shows the errors it recovered from first.
    let list = list.replace("%recover close: ()\n", "");
    let paths = files("recover_fails", &[("list.fpeg", &list)]);
    let args = ["parse".as_ref(), "--recover".as_ref(), paths[0].as_os_str()];
    let out = run_with_input(args, "[1, x");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert_eq!(
        text(&out.stderr),
        "error[item]: expected N\n --> <stdin>:1:5\n  |\n1 | [1, x\n  |     ^\n\n\
         error[close]: expected \",\", \"]\"\n --> <stdin>:1:6\n  |\n1 | [1, x\n  |      ^\n\n"
    );
}

#[test]
fn a_closed_output_is_no_failure_of_the_command() {
    // Fifty copies of a grammar with a hundred problems print more than a pipe holds.
    let unknown: Vec<String> = (0..100).map(|i| format!("missing{i}")).collect();
    let paths = files(
        "closed",
        &[("many.fpeg", &format!("start: {}\n", unknown.join(" ")))],
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_fpeg"))
        .arg("check")
        .args(std::iter::repeat_n(paths[0].as_os_str(), 50))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the fpeg binary runs");
    drop(child.stdout.take());
    let out = child.wait_with_output().expect("the binary finishes");
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
}

#[test]
fn an_unknown_start_rule_and_a_byte_order_mark_are_problems_of_the_grammar_file() {
    let paths = files(
        "start",
        &[
            ("marked.fpeg", "\u{feff}start: \"a\"\n"),
            ("input.txt", "a"),
        ],
    );
    let out = run(["check".as_ref(), paths[0].as_os_str()]);
    assert!(out.status.success(), "{}", text(&out.stdout));
    let out = run([
        "parse".as_ref(),
        "--start".as_ref(),
        "nope".as_ref(),
        paths[0].as_os_str(),
        paths[1].as_os_str(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        text(&out.stderr),
        format!(
            "{}: error: no rule nope to start from\n",
            paths[0].display()
        )
    );
}

#[test]
fn check_and_parse_report_each_problem_with_its_place_and_code() {
    let paths = files(
        "check",
        &[
            ("bad.fpeg", "start: x\nstart: \"a\"\n"),
            ("good.fpeg", SUMS),
        ],
    );
    let out = run(["check".as_ref(), paths[1].as_os_str()]);
    assert!(out.status.success(), "{}", text(&out.stdout));
    assert!(out.stdout.is_empty());
    let out = run(["check".as_ref(), paths[0].as_os_str(), paths[1].as_os_str()]);
    assert_eq!(out.status.code(), Some(1));
    let bad = paths[0].display();
    let found = format!(
        "{bad}:1:8: error[E201]: unknown rule x\n\
         {bad}:2:1: error[E204]: the rule start is defined twice\n"
    );
    assert_eq!(text(&out.stdout), found);
    // `parse` prints them as `check` does, in the order of the text and not in the one the
    // resolver meets them in.
    let out = run(["parse".as_ref(), paths[0].as_os_str(), paths[1].as_os_str()]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), found);
}

#[test]
fn a_grammar_extends_and_imports_the_grammar_files_beside_it() {
    let paths = files(
        "modules",
        &[
            (
                "base.fpeg",
                "start: item+\nitem: WORD\nWORD: [a-z]+\n%ignore \" \"+\n",
            ),
            ("main.fpeg", "%extends \"base\"\nitem: ... | N\nN: [0-9]+\n"),
            ("input.txt", "ab 12"),
            ("broken.fpeg", "start: x\n"),
            ("user.fpeg", "%import broken (start)\n"),
        ],
    );
    let out = run(["parse".as_ref(), paths[1].as_os_str(), paths[2].as_os_str()]);
    assert!(out.status.success(), "{}", text(&out.stdout));
    assert_eq!(
        text(&out.stdout),
        "start\n  item\n    WORD\"ab\"\n  item\n    N\"12\"\n"
    );
    // A problem in a module stands at the directive, with its place in the module.
    let out = run(["check".as_ref(), paths[4].as_os_str()]);
    assert_eq!(out.status.code(), Some(1));
    let user = paths[4].display();
    let found = format!(
        "{user}:1:9: error[E201]: in the grammar broken, line 1, column 8: unknown rule x\n"
    );
    assert_eq!(text(&out.stdout), found);
}

#[test]
fn check_prints_warnings_and_fails_on_them_only_when_strict() {
    let paths = files(
        "warnings",
        &[("odd.fpeg", "start: \"a\"? | \"b\"\n"), ("good.fpeg", SUMS)],
    );
    let odd = paths[0].display();
    let warning = format!(
        "{odd}:1:15: warning[W101]: this alternative is never tried, since the one before it \
         cannot fail\n"
    );
    let out = run(["check".as_ref(), paths[0].as_os_str()]);
    assert!(out.status.success());
    assert_eq!(text(&out.stdout), warning);
    let out = run([
        "check".as_ref(),
        paths[1].as_os_str(),
        "--strict".as_ref(),
        paths[0].as_os_str(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), warning);
    let out = run(["check".as_ref(), "--strict".as_ref(), paths[1].as_os_str()]);
    assert!(out.status.success(), "{}", text(&out.stdout));
}

/// The cells of the line of a profile's table that starts with `name`.
fn profile_row(table: &str, name: &str) -> Vec<String> {
    let line = table.lines().find(|line| line.starts_with(name));
    let line = line.unwrap_or_else(|| panic!("no row {name} in\n{table}"));
    line.split_whitespace().map(str::to_owned).collect()
}

#[test]
fn profile_prints_a_row_for_each_rule_and_the_ignored_text() {
    let paths = files(
        "profile",
        &[("sums.fpeg", SUMS), ("a.txt", "1 + 23"), ("b.txt", "4")],
    );
    let out = run([
        "profile".as_ref(),
        paths[0].as_os_str(),
        paths[1].as_os_str(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let table = text(&out.stdout);
    assert!(table.starts_with("rule "), "{table}");
    // The name, the calls and the failures.
    assert_eq!(profile_row(&table, "N ")[..3], ["N", "2", "0"]);
    assert_eq!(profile_row(&table, "start")[1], "1");
    assert!(table.contains("\n%ignore ") && table.contains("\n(parse) "));
    assert!(table.contains(" steps, at most "), "{table}");
    // Two inputs add up.
    let out = run([
        "profile".as_ref(),
        paths[0].as_os_str(),
        paths[1].as_os_str(),
        paths[2].as_os_str(),
    ]);
    assert_eq!(profile_row(&text(&out.stdout), "N ")[1], "3");
}

#[test]
fn profile_suggests_remembering_a_rule_that_is_matched_again_and_again() {
    // `item` is tried again through `dot` and `bang`, which remembering by default does not
    // look through.
    let grammar = "start: dot | bang | item\ndot: item \".\"\nbang: item \"!\"\n\
                   item: \"(\" start \")\" | \"x\"\n";
    let paths = files(
        "suggest",
        &[("again.fpeg", grammar), ("input.txt", "((((((x))))))")],
    );
    let out = run([
        "profile".as_ref(),
        paths[0].as_os_str(),
        paths[1].as_os_str(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stdout).contains("\n@memo on item: "),
        "{}",
        text(&out.stdout)
    );
    // Remembered, the rule is no longer worth a suggestion.
    let marked = grammar.replace("item:", "@memo\nitem:");
    let paths = files(
        "suggested",
        &[("again.fpeg", &marked), ("input.txt", "((((((x))))))")],
    );
    let out = run([
        "profile".as_ref(),
        paths[0].as_os_str(),
        paths[1].as_os_str(),
    ]);
    assert!(
        !text(&out.stdout).contains("@memo on"),
        "{}",
        text(&out.stdout)
    );
    // Nor is a rule whose repeated calls cost little, since a remembered rule they call
    // answers them, however much its first call cost.
    let cheap =
        "start: a | b\na: w \"!\"\nb: w \"?\"\nw: \"x\"? m\n@memo\nm: \"(\" m \")\" | \"y\"\n";
    let nested = format!("{}y{}?", "(".repeat(40), ")".repeat(40));
    let paths = files("cheap", &[("again.fpeg", cheap), ("input.txt", &nested)]);
    let out = run([
        "profile".as_ref(),
        paths[0].as_os_str(),
        paths[1].as_os_str(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        !text(&out.stdout).contains("@memo on w:"),
        "{}",
        text(&out.stdout)
    );
    // Nor is a rule of the ignored text, which the profile skips again where a parse outside
    // it ends the skip from where the last one started.
    let comments = format!("{grammar}COMMENT: \"#\" [^\\n]* \"\\n\"\n%ignore \" \" | COMMENT\n");
    let lines = "( # a comment long enough to cost steps when it is skipped again\n".repeat(4);
    let input = format!("{lines}x))))");
    let paths = files(
        "comments",
        &[("again.fpeg", &comments), ("input.txt", &input)],
    );
    let out = run([
        "profile".as_ref(),
        paths[0].as_os_str(),
        paths[1].as_os_str(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("\n@memo on item: ") && !stdout.contains("@memo on COMMENT:"),
        "{stdout}"
    );
    // Nor is a rule marked @nomemo, nor a precedence block or its levels, which @memo cannot
    // stand on. The start rule, called inside them, may be.
    let unmarked = grammar.replace("item:", "@nomemo\nitem:");
    let block = "start: e \".\" | e \"!\" | e\n@nomemo\ne: %precedence {\n    \
                 sum: left=sum \"+\" right=atom -> add\n    atom: \"(\" start \")\" | \"x\"\n}\n";
    for (name, grammar, names) in [
        ("unmarked", unmarked.as_str(), &["item"][..]),
        ("block", block, &["e", "sum", "atom"][..]),
    ] {
        let paths = files(
            name,
            &[("again.fpeg", grammar), ("input.txt", "((((((x))))))")],
        );
        let out = run([
            "profile".as_ref(),
            paths[0].as_os_str(),
            paths[1].as_os_str(),
        ]);
        assert!(out.status.success(), "{}", text(&out.stderr));
        for rule in names {
            let line = format!("@memo on {rule}:");
            assert!(
                !text(&out.stdout).contains(&line),
                "{name}: {}",
                text(&out.stdout)
            );
        }
    }
    // Nor is a rule that an import brings in under a private name, which no grammar can write.
    let paths = files(
        "imported",
        &[
            ("again.fpeg", "%import rep (start)\n"),
            ("input.txt", "((((((x))))))"),
            ("rep.fpeg", grammar),
        ],
    );
    let out = run([
        "profile".as_ref(),
        paths[0].as_os_str(),
        paths[1].as_os_str(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        !text(&out.stdout).contains("@memo on rep.item:"),
        "{}",
        text(&out.stdout)
    );
}

#[test]
fn profile_goes_on_past_an_unreadable_input_and_stops_at_an_unknown_start() {
    let paths = files(
        "profile_inputs",
        &[("sums.fpeg", SUMS), ("good.txt", "1 + 2")],
    );
    let missing = paths[1].with_file_name("missing.txt");
    let out = run([
        "profile".as_ref(),
        paths[0].as_os_str(),
        missing.as_os_str(),
        paths[1].as_os_str(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).starts_with(&format!("{}: error: cannot read it", missing.display()))
    );
    assert!(
        text(&out.stdout).contains("\nstart "),
        "{}",
        text(&out.stdout)
    );
    // With no input read there is no table.
    let out = run([
        "profile".as_ref(),
        paths[0].as_os_str(),
        missing.as_os_str(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "{}", text(&out.stdout));
    let out = run([
        "profile".as_ref(),
        "--start".as_ref(),
        "nope".as_ref(),
        paths[0].as_os_str(),
        paths[1].as_os_str(),
        paths[1].as_os_str(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        text(&out.stderr),
        format!(
            "{}: error: no rule nope to start from\n",
            paths[0].display()
        )
    );
    assert!(out.stdout.is_empty(), "{}", text(&out.stdout));
}

#[test]
fn profile_prints_json_and_reports_an_input_that_does_not_parse() {
    let paths = files("profile_json", &[("sums.fpeg", SUMS), ("bad.txt", "1 +")]);
    let out = run([
        "profile".as_ref(),
        "--json".as_ref(),
        paths[0].as_os_str(),
        paths[1].as_os_str(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        text(&out.stderr),
        format!(
            "error: expected N\n --> {}:1:4\n  |\n1 | 1 +\n  |    ^\n\n",
            paths[1].display()
        )
    );
    let json = text(&out.stdout);
    assert!(json.starts_with("{\"steps\":"), "{json}");
    assert!(json.contains("{\"name\":\"N\",\"remembered\":false,\"calls\":2,\"failures\":1,"));
    assert!(json.ends_with("]}\n"), "{json}");
}

#[test]
fn profile_needs_a_grammar_and_an_input() {
    let paths = files("profile_usage", &[("sums.fpeg", SUMS)]);
    let out = run(["profile".as_ref(), paths[0].as_os_str()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).starts_with("error: profile needs a grammar and an input\n"));
}
