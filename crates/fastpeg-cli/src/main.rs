// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! The `fastpeg` command.

use std::process::ExitCode;

const USAGE: &str = "\
Usage: fastpeg [--help | --version]

The commands check, parse and profile arrive with the parsing engine.";

/// Exit status for a command line that cannot be understood, as in most Unix tools.
const EXIT_USAGE: u8 = 2;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["--version" | "-V"] => {
            println!("fastpeg {}", fastpeg::VERSION);
            ExitCode::SUCCESS
        }
        ["--help" | "-h"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        [] => {
            eprintln!("{USAGE}");
            ExitCode::from(EXIT_USAGE)
        }
        [first, ..] => {
            eprintln!("error: unexpected argument '{first}'\n\n{USAGE}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}
