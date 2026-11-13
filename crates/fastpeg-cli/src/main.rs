// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `fastpeg` command.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use fastpeg::{Grammar, GrammarError, LineIndex, NodeRef, ParseErrorKind, Parser, Tree};

const USAGE: &str = "\
Usage: fastpeg <command> [options]

Commands:
  check GRAMMAR...         report the problems of grammars
  parse GRAMMAR [INPUT]    parse INPUT, or standard input, and print its tree

Options of parse:
  --start RULE             the rule to start from, start if not given
  --spans                  show the byte span of every node and token
  --json                   print the tree as JSON, spans included

  -h, --help               print this help
  -V, --version            print the version";

/// Exit status when a grammar has problems or an input does not parse.
const EXIT_PROBLEM: u8 = 1;

/// Exit status for a command line that cannot be understood, as in most Unix tools.
const EXIT_USAGE: u8 = 2;

/// Print a line on standard output, or on standard error with `error`. A stream that closed,
/// as when a pager quits, is no failure of the command, so what cannot be written is dropped.
fn say(error: bool, line: std::fmt::Arguments<'_>) {
    let _ = match error {
        true => writeln!(std::io::stderr(), "{line}"),
        false => writeln!(std::io::stdout(), "{line}"),
    };
}

/// Why a command stopped: problems already reported, or a command line it cannot understand.
enum Failure {
    Problem,
    Usage(String),
}

fn main() -> ExitCode {
    // args_os, since std::env::args panics on an argument that is not valid Unicode.
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let outcome = match args.first().map(|arg| arg.to_string_lossy()).as_deref() {
        Some("--version" | "-V") if args.len() == 1 => {
            say(false, format_args!("fastpeg {}", fastpeg::VERSION));
            Ok(())
        }
        Some("--help" | "-h") if args.len() == 1 => {
            say(false, format_args!("{USAGE}"));
            Ok(())
        }
        Some("check") => check(&args[1..]),
        Some("parse") => parse(&args[1..]),
        None => Err(Failure::Usage(String::new())),
        Some(first) => {
            // A known flag takes no second argument, so then the second is the culprit.
            let flag = ["--version", "-V", "--help", "-h"].contains(&first);
            let culprit = args.get(usize::from(flag)).map(|a| a.to_string_lossy());
            Err(Failure::Usage(format!(
                "unexpected argument '{}'",
                culprit.unwrap_or_default()
            )))
        }
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Problem) => ExitCode::from(EXIT_PROBLEM),
        Err(Failure::Usage(message)) => {
            if !message.is_empty() {
                say(true, format_args!("error: {message}\n"));
            }
            say(true, format_args!("{USAGE}"));
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// Read a file as UTF-8 text, reporting why it cannot be.
fn read(path: &Path) -> Result<String, Failure> {
    let bytes = std::fs::read(path).map_err(|error| {
        say(
            true,
            format_args!("{}: error: cannot read it: {error}", path.display()),
        );
        Failure::Problem
    })?;
    String::from_utf8(bytes).map_err(|_| {
        say(
            true,
            format_args!("{}: error: it is not UTF-8 text", path.display()),
        );
        Failure::Problem
    })
}

/// Print the problems of a grammar, each with its place in the grammar text.
fn report(path: &Path, text: &str, errors: &[GrammarError]) {
    let lines = LineIndex::new(text);
    for error in errors {
        let (line, column) = lines.line_column(text, error.span.start as usize);
        let (code, message) = (error.code, &error.message);
        say(
            false,
            format_args!(
                "{}:{line}:{column}: error[{code}]: {message}",
                path.display()
            ),
        );
    }
}

/// Read and compile a grammar, or report its problems.
fn compile(path: &Path) -> Result<Parser, Failure> {
    let file = read(path)?;
    // Editors on Windows may begin a file with a byte order mark, which is no part of the text.
    let text = file.strip_prefix('\u{feff}').unwrap_or(&file);
    let grammar = Grammar::from_text(text).map_err(|errors| {
        report(path, text, &errors);
        Failure::Problem
    })?;
    Parser::new(&grammar).map_err(|errors| {
        report(path, text, &errors);
        Failure::Problem
    })
}

fn check(args: &[OsString]) -> Result<(), Failure> {
    if args.is_empty() {
        return Err(Failure::Usage("check needs a grammar".into()));
    }
    if let Some(option) = args.iter().find(|a| a.to_string_lossy().starts_with('-')) {
        let option = option.to_string_lossy();
        return Err(Failure::Usage(format!("unexpected argument '{option}'")));
    }
    let mut problems = false;
    for path in args {
        problems |= compile(&PathBuf::from(path)).is_err();
    }
    match problems {
        true => Err(Failure::Problem),
        false => Ok(()),
    }
}

/// The settings of `parse`.
#[derive(Default)]
struct ParseOptions {
    start: Option<String>,
    spans: bool,
    json: bool,
    paths: Vec<OsString>,
}

fn parse_options(args: &[OsString]) -> Result<ParseOptions, Failure> {
    let mut options = ParseOptions::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--start") => match args.next().and_then(|rule| rule.to_str()) {
                Some(rule) => options.start = Some(rule.to_owned()),
                None => return Err(Failure::Usage("--start needs a rule".into())),
            },
            Some("--spans") => options.spans = true,
            Some("--json") => options.json = true,
            Some(option) if option.starts_with('-') && option != "-" => {
                return Err(Failure::Usage(format!("unexpected argument '{option}'")));
            }
            _ => options.paths.push(arg.clone()),
        }
    }
    if options.paths.is_empty() || options.paths.len() > 2 {
        return Err(Failure::Usage(
            "parse needs a grammar and at most one input".into(),
        ));
    }
    Ok(options)
}

fn parse(args: &[OsString]) -> Result<(), Failure> {
    let options = parse_options(args)?;
    let grammar = PathBuf::from(&options.paths[0]);
    let parser = compile(&grammar)?;
    let (name, input) = match options.paths.get(1).filter(|path| *path != "-") {
        Some(path) => (
            path.to_string_lossy().into_owned(),
            read(&PathBuf::from(path))?,
        ),
        None => {
            let mut bytes = Vec::new();
            let name = "<stdin>".to_owned();
            if let Err(error) = std::io::stdin().read_to_end(&mut bytes) {
                say(true, format_args!("{name}: error: cannot read it: {error}"));
                return Err(Failure::Problem);
            }
            match String::from_utf8(bytes) {
                Ok(input) => (name, input),
                Err(_) => {
                    say(true, format_args!("{name}: error: it is not UTF-8 text"));
                    return Err(Failure::Problem);
                }
            }
        }
    };
    let start = options.start.as_deref().unwrap_or("start");
    match parser.parse(&input, start) {
        Ok(tree) => {
            let text = match options.json {
                true => json(&tree, &input),
                false => tree.to_lines(&input, options.spans),
            };
            // A closed pipe is no failure of the parse.
            let _ = std::io::stdout().write_all(text.as_bytes());
            Ok(())
        }
        // A start rule the grammar lacks is a problem of the grammar, not of the input.
        Err(error) if matches!(error.kind, ParseErrorKind::UnknownStart(_)) => {
            let message = error.message();
            say(
                true,
                format_args!("{}: error: {message}", grammar.display()),
            );
            Err(Failure::Problem)
        }
        Err(error) => {
            let (line, column) = LineIndex::new(&input).line_column(&input, error.position);
            say(
                true,
                format_args!("{name}:{line}:{column}: error: {}", error.message()),
            );
            Err(Failure::Problem)
        }
    }
}

/// The tree as JSON: an array of the top items, a node as an object with its kind, field,
/// span and children, a token with its kind, field, span, text and whether it is hidden.
/// Spans are byte offsets.
fn json(tree: &Tree, input: &str) -> String {
    /// What is left to write: an item, or the end of a node's children.
    enum Step<'t> {
        Item(NodeRef<'t>, bool),
        Close,
    }
    let mut out = String::from("[");
    let mut pending: Vec<Step<'_>> = Vec::new();
    for (i, root) in tree.roots().into_iter().enumerate().rev() {
        pending.push(Step::Item(root, i > 0));
    }
    while let Some(step) = pending.pop() {
        let (item, comma) = match step {
            Step::Close => {
                out.push_str("]}");
                continue;
            }
            Step::Item(item, comma) => (item, comma),
        };
        if comma {
            out.push(',');
        }
        out.push_str("{\"kind\":");
        push_json_string(&mut out, item.kind_name());
        if let Some(field) = item.field_name() {
            out.push_str(",\"field\":");
            push_json_string(&mut out, field);
        }
        let (start, end) = item.span();
        let _ = write!(out, ",\"start\":{start},\"end\":{end}");
        if item.is_token() {
            out.push_str(",\"text\":");
            push_json_string(&mut out, item.text(input));
            if item.is_hidden() {
                out.push_str(",\"hidden\":true");
            }
            out.push('}');
        } else {
            out.push_str(",\"children\":[");
            pending.push(Step::Close);
            for (i, child) in item.children().into_iter().enumerate().rev() {
                pending.push(Step::Item(child, i > 0));
            }
        }
    }
    out.push_str("]\n");
    out
}

fn push_json_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
