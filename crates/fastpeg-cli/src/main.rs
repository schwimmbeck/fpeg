// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! The `fastpeg` command.

use std::ffi::OsString;
use std::process::ExitCode;

const USAGE: &str = "\
Usage: fastpeg [--help | --version]

The commands check, parse and profile arrive with the parsing engine.";

/// Exit status for a command line that cannot be understood, as in most Unix tools.
const EXIT_USAGE: u8 = 2;

const FLAGS: [&str; 4] = ["--version", "-V", "--help", "-h"];

fn main() -> ExitCode {
    // args_os, since std::env::args panics on an argument that is not valid Unicode.
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let first = args.first().map(|arg| arg.to_string_lossy());
    match (first.as_deref(), args.len()) {
        (Some("--version" | "-V"), 1) => {
            println!("fastpeg {}", fastpeg::VERSION);
            ExitCode::SUCCESS
        }
        (Some("--help" | "-h"), 1) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        (None, _) => {
            eprintln!("{USAGE}");
            ExitCode::from(EXIT_USAGE)
        }
        (Some(first), _) => {
            // Only one flag is accepted, so after a known flag the second argument is the culprit.
            let unexpected = usize::from(FLAGS.contains(&first));
            if let Some(arg) = args.get(unexpected) {
                eprintln!("error: unexpected argument '{}'", arg.to_string_lossy());
            }
            eprintln!("\n{USAGE}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}
