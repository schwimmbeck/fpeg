// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! Runs the built `fastpeg` binary.

use std::ffi::{OsStr, OsString};
use std::process::{Command, Output};

fn run<I: IntoIterator<Item = S>, S: AsRef<OsStr>>(args: I) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fastpeg"))
        .args(args)
        .output()
        .expect("the fastpeg binary runs")
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
    let out = run(["--version", "--no-such-flag"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unexpected argument '--no-such-flag'"));
}

#[test]
fn argument_that_is_not_unicode_is_a_usage_error() {
    let out = run([not_unicode()]);
    assert_eq!(out.status.code(), Some(2));
}
