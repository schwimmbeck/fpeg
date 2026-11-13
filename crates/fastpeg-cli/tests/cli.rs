// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Runs the built `fastpeg` binary.

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
    let mut child = Command::new(env!("CARGO_BIN_EXE_fastpeg"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the fastpeg binary runs");
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
        format!("fastpeg {}\n", fastpeg::VERSION)
    );
}

#[test]
fn unaccepted_argument_is_a_usage_error_that_names_it() {
    for args in [
        &["--version", "--no-such-flag"][..],
        &["parse", "--no-such-flag"],
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
fn a_failed_parse_reports_where_and_what_was_expected() {
    let paths = files("failed", &[("sums.fpeg", SUMS)]);
    let out = run_with_input(["parse".as_ref(), paths[0].as_os_str()], "1 +\n+ 2");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    // Only spaces are ignored, so the line break is where an N was expected.
    assert_eq!(text(&out.stderr), "<stdin>:1:4: error: expected N\n");
}

#[test]
fn check_reports_each_problem_with_its_place_and_code() {
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
    assert_eq!(
        text(&out.stdout),
        format!(
            "{bad}:2:1: error[E204]: the rule start is defined twice\n\
             {bad}:1:8: error[E201]: unknown rule x\n"
        )
    );
}
