// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `fpeg` command.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use fpeg::{
    Diagnostic, Grammar, LineIndex, ModuleFiles, NodeRef, ParseError, ParseErrorKind, Parser,
    Severity, Source, Tree, visible,
};

mod profile;

const USAGE: &str = "\
Usage: fpeg <command> [options]

Options:
  -h, --help                   print this help
  -V, --version                print the version

Commands:
  check GRAMMAR...             report the errors and warnings of grammars
  parse GRAMMAR [INPUT]        parse INPUT, or standard input, and print its tree
  profile GRAMMAR INPUT...     parse the inputs and print what each rule did

Options of check:
  --strict                     fail on a warning as on an error

Options of parse and profile:
  --start RULE                 the rule to start from, start if not given
  --json                       print the tree, or the counts of the rules, as JSON

Options of parse:
  --spans                      show the byte span of every node and token
  --recover                    go on past errors where the grammar has recoveries";

/// Exit status when a grammar has problems or an input does not parse.
const EXIT_PROBLEM: u8 = 1;

/// Exit status for a command line that cannot be understood, as in most Unix tools.
const EXIT_USAGE: u8 = 2;

/// The most bytes of indentation a tree printed by lines may take. Left recursion makes trees as
/// deep as their input is long, so the indentation grows with the square of the input.
const MAX_INDENTATION_BYTES: usize = 1 << 28;

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
    // An argument that starts with a dash is an option wherever it stands, so help may follow
    // a command and its arguments.
    let help = args.iter().any(|arg| arg == "--help" || arg == "-h");
    let outcome = match args.first().map(|arg| arg.to_string_lossy()).as_deref() {
        _ if help => {
            say(false, format_args!("{USAGE}"));
            Ok(())
        }
        Some("--version" | "-V") if args.len() == 1 => {
            say(false, format_args!("fpeg {}", fpeg::VERSION));
            Ok(())
        }
        Some("check") => check(&args[1..]),
        Some("parse") => parse(&args[1..]),
        Some("profile") => profile::profile(&args[1..]),
        None => Err(Failure::Usage(String::new())),
        Some(first) => {
            // A known flag takes no second argument, so then the second is the culprit.
            let flag = ["--version", "-V"].contains(&first);
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
    utf8(bytes, &path.display().to_string())
}

/// The text of what was read from `name`, or a report that it is no UTF-8. Editors on Windows
/// may begin a file with a byte order mark, which is no part of the text.
fn utf8(bytes: Vec<u8>, name: &str) -> Result<String, Failure> {
    let Ok(mut text) = String::from_utf8(bytes) else {
        say(true, format_args!("{name}: error: it is not UTF-8 text"));
        return Err(Failure::Problem);
    };
    if text.starts_with('\u{feff}') {
        text.drain(..'\u{feff}'.len_utf8());
    }
    Ok(text)
}

/// Print the errors and warnings of a grammar, each with its place in the grammar text.
fn report(path: &Path, text: &str, found: &[Diagnostic]) {
    let lines = LineIndex::new(text);
    for diagnostic in found {
        let (line, column) = lines.line_column(text, diagnostic.span.start as usize);
        // A message may quote the grammar, which the console must not act on.
        let (severity, code, message) = (
            diagnostic.severity(),
            diagnostic.code,
            visible(&diagnostic.message),
        );
        say(
            false,
            format_args!(
                "{}:{line}:{column}: {severity}[{code}]: {message}",
                path.display()
            ),
        );
    }
}

/// Read a grammar and join it with the grammars it extends and imports, from the files beside it
/// before those that come with fpeg.
fn joined(path: &Path, text: &str) -> Result<Grammar, Vec<Diagnostic>> {
    let beside = ModuleFiles::new(path.parent().unwrap_or(Path::new("")));
    Grammar::from_text(text)?.link(&beside)
}

/// Read and compile a grammar, or report its errors.
fn compile(path: &Path) -> Result<(Grammar, Parser), Failure> {
    let text = read(path)?;
    let grammar = joined(path, &text).map_err(|errors| {
        report(path, &text, &errors);
        Failure::Problem
    })?;
    let parser = Parser::new(&grammar).map_err(|errors| {
        report(path, &text, &errors);
        Failure::Problem
    })?;
    Ok((grammar, parser))
}

fn check(args: &[OsString]) -> Result<(), Failure> {
    let mut strict = false;
    let mut paths = Vec::new();
    for arg in args {
        match arg.to_string_lossy().as_ref() {
            "--strict" => strict = true,
            option if option.starts_with('-') => {
                return Err(Failure::Usage(format!("unexpected argument '{option}'")));
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    if paths.is_empty() {
        return Err(Failure::Usage("check needs a grammar".into()));
    }
    let mut failed = false;
    for path in &paths {
        let Ok(text) = read(path) else {
            failed = true;
            continue;
        };
        let found = match joined(path, &text) {
            Ok(grammar) => fpeg::check(&grammar),
            Err(errors) => errors,
        };
        report(path, &text, &found);
        failed |= (found.iter()).any(|found| strict || found.severity() == Severity::Error);
    }
    match failed {
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
    recover: bool,
    paths: Vec<OsString>,
}

/// The rule that follows `--start`. One that is not Unicode can name no rule, which the parse
/// then reports as for any rule the grammar lacks.
fn start_rule(arg: Option<&OsString>) -> Result<String, Failure> {
    match arg {
        Some(rule) => Ok(rule.to_string_lossy().into_owned()),
        None => Err(Failure::Usage("--start needs a rule".into())),
    }
}

fn parse_options(args: &[OsString]) -> Result<ParseOptions, Failure> {
    let mut options = ParseOptions::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--start") => options.start = Some(start_rule(args.next())?),
            Some("--spans") => options.spans = true,
            Some("--json") => options.json = true,
            Some("--recover") => options.recover = true,
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
    let (_, parser) = compile(&grammar)?;
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
            let input = utf8(bytes, &name)?;
            (name, input)
        }
    };
    let start = options.start.as_deref().unwrap_or("start");
    let parsed = match options.recover {
        true => parser.parse_recovering(&input, start),
        false => parser.parse(&input, start),
    };
    match parsed {
        Ok(tree) => {
            if !options.json && indentation_bytes(&tree) > MAX_INDENTATION_BYTES {
                let mib = MAX_INDENTATION_BYTES >> 20;
                say(
                    true,
                    format_args!(
                        "{name}: error: the tree nests too deeply to print by lines in {mib} MiB, \
                         which --json does"
                    ),
                );
                return Err(Failure::Problem);
            }
            let text = match options.json {
                true => json(&tree, &input),
                false => tree.to_lines(&input, options.spans),
            };
            // A closed pipe is no failure of the parse.
            let _ = std::io::stdout().write_all(text.as_bytes());
            if tree.recoveries().is_empty() {
                return Ok(());
            }
            let source = Source::new(&input, Some(&name));
            for recovery in tree.recoveries() {
                say(true, format_args!("{}\n", recovery.render(&source)));
            }
            Err(Failure::Problem)
        }
        Err(error) => {
            report_failure(&grammar, &name, &input, &error);
            Err(Failure::Problem)
        }
    }
}

/// Report an input that did not parse: each error with the line it stands in, and a blank line
/// after it. A start rule the grammar lacks is a problem of the grammar, not of the input.
fn report_failure(grammar: &Path, name: &str, input: &str, error: &ParseError) {
    if matches!(error.kind, ParseErrorKind::UnknownStart(_)) {
        let message = error.message();
        say(
            true,
            format_args!("{}: error: {message}", grammar.display()),
        );
        return;
    }
    let source = Source::new(input, Some(name));
    say(true, format_args!("{}\n", error.render(&source)));
}

/// The bytes of indentation of a tree printed by lines: two for each node an item lies in.
fn indentation_bytes(tree: &Tree) -> usize {
    let mut total = 0usize;
    let mut pending: Vec<(NodeRef<'_>, usize)> =
        (tree.roots().into_iter()).map(|root| (root, 0)).collect();
    while let Some((item, depth)) = pending.pop() {
        total = total.saturating_add(2 * depth);
        pending.extend(item.children().into_iter().map(|child| (child, depth + 1)));
    }
    total
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
            c if acted_on(c) => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Whether a terminal acts on a character rather than showing it: a control, DEL, a
/// bidirectional control, or a line or paragraph separator. JSON may escape any character.
fn acted_on(c: char) -> bool {
    matches!(
        c,
        '\0'..='\x1f'
            | '\x7f'..='\u{9f}'
            | '\u{61c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}
