// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! Runs the built `fastpeg` binary.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fastpeg"))
        .args(args)
        .output()
        .expect("the fastpeg binary runs")
}

#[test]
fn version_flag_prints_the_core_version() {
    let out = run(&["--version"]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("fastpeg {}\n", fastpeg::VERSION)
    );
}

#[test]
fn unknown_argument_is_a_usage_error() {
    let out = run(&["--no-such-flag"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("'--no-such-flag'"));
}
