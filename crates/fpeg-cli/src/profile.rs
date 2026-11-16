// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `fpeg profile`: what each rule of a grammar did while it parsed inputs.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;

use fpeg::{Grammar, Limits, ParseErrorKind, Profile, ProfileRow, RuleBody};

use crate::{Failure, compile, push_json_string, read, report_failure, start_rule};

/// The share of all steps, in percent, that remembering a rule must be worth before the
/// profile suggests it.
const SUGGEST_MEMO_PERCENT: u64 = 5;

/// A count of a row: the heading of its column in the table, the name of its member in the
/// JSON, and where a row holds it.
struct Count {
    heading: &'static str,
    member: &'static str,
    of: fn(&ProfileRow) -> u64,
}

/// The counts of a row in the order the table and the JSON show them.
const COUNTS: [Count; 9] = [
    count("calls", "calls", |row| row.calls),
    count("failed", "failures", |row| row.failures),
    count("steps", "steps", |row| row.steps),
    count("with callees", "steps_with_callees", |row| {
        row.steps_with_callees
    }),
    count("given back", "backtracked_bytes", |row| {
        row.backtracked_bytes
    }),
    count("repeats", "repeats", |row| row.repeats),
    count("hits", "memo_hits", |row| row.memo_hits),
    count("stored", "memo_stores", |row| row.memo_stores),
    count("grown", "growths", |row| row.growths),
];

const fn count(heading: &'static str, member: &'static str, of: fn(&ProfileRow) -> u64) -> Count {
    Count {
        heading,
        member,
        of,
    }
}

pub(crate) fn profile(args: &[OsString]) -> Result<(), Failure> {
    let (mut start, mut json, mut paths) = (None, false, Vec::new());
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--start") => start = Some(start_rule(args.next())?),
            Some("--json") => json = true,
            Some(option) if option.starts_with('-') => {
                return Err(Failure::Usage(format!("unexpected argument '{option}'")));
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    let Some((grammar, inputs)) = paths.split_first().filter(|(_, inputs)| !inputs.is_empty())
    else {
        return Err(Failure::Usage(
            "profile needs a grammar and an input".into(),
        ));
    };
    let (model, parser) = compile(grammar)?;
    let start = start.as_deref().unwrap_or("start");
    let mut total = Profile::default();
    let (mut failed, mut parsed) = (false, 0);
    for path in inputs {
        let Ok(input) = read(path) else {
            failed = true;
            continue;
        };
        let (result, profile) = parser.profile(&input, start, Limits::default());
        if let Err(error) = result {
            report_failure(grammar, &path.to_string_lossy(), &input, &error);
            // A start rule that the grammar lacks fails every input alike.
            if matches!(error.kind, ParseErrorKind::UnknownStart(_)) {
                return Err(Failure::Problem);
            }
            failed = true;
        }
        total.add(profile);
        parsed += 1;
    }
    let text = match (parsed, json) {
        (0, _) => String::new(),
        (_, true) => as_json(&total),
        (_, false) => table(&total, &unmarkable(&model)),
    };
    // A closed pipe is no failure of the command.
    let _ = std::io::stdout().write_all(text.as_bytes());
    match failed {
        true => Err(Failure::Problem),
        false => Ok(()),
    }
}

/// The names that `@memo` cannot stand on, or where it would contradict the rule's mark: the
/// levels of precedence blocks, the blocks, whose marks are for their levels, the rules marked
/// `@nomemo`, and the rules an import brings in under private names, which no grammar can write.
fn unmarkable(grammar: &Grammar) -> HashSet<&str> {
    let mut names = HashSet::new();
    for rule in &grammar.rules {
        if rule.name.text.contains('.') {
            names.insert(rule.name.text.as_str());
        }
        match &rule.body {
            RuleBody::Precedence(levels) => {
                names.insert(rule.name.text.as_str());
                names.extend(levels.iter().map(|level| level.name.text.as_str()));
            }
            RuleBody::Alternatives(_) if rule.has_attribute("nomemo") => {
                names.insert(rule.name.text.as_str());
            }
            RuleBody::Alternatives(_) => {}
        }
    }
    names
}

/// The rules a parse reached, those with the most steps of their own first.
fn reached(profile: &Profile) -> Vec<&ProfileRow> {
    let mut rows: Vec<&ProfileRow> = (profile.rows.iter())
        .filter(|row| row.calls > 0 || row.steps > 0)
        .collect();
    rows.sort_by(|a, b| b.steps.cmp(&a.steps).then_with(|| a.name.cmp(&b.name)));
    rows
}

/// The profile as a table, a line of totals, and what it suggests, which is never `@memo` on
/// a name in `unmarkable`.
fn table(profile: &Profile, unmarkable: &HashSet<&str>) -> String {
    let rows = reached(profile);
    let mut lines: Vec<Vec<String>> = Vec::new();
    // The share of all steps stands right after the steps, past the column of the rule.
    let share_at = 2 + COUNTS
        .iter()
        .take_while(|count| count.member != "steps")
        .count();
    let mut headings = vec!["rule".to_owned()];
    headings.extend(COUNTS.iter().map(|count| count.heading.to_owned()));
    headings.insert(share_at, "share".to_owned());
    lines.push(headings);
    for row in &rows {
        let mut cells = vec![row.name.clone()];
        cells.extend(COUNTS.iter().map(|count| (count.of)(row).to_string()));
        let share = 100.0 * row.steps as f64 / profile.steps.max(1) as f64;
        cells.insert(share_at, format!("{share:.1}%"));
        lines.push(cells);
    }
    let width = |column: usize| {
        let cells = lines.iter().filter_map(|cells| cells.get(column));
        cells.map(|cell| cell.chars().count()).max().unwrap_or(0)
    };
    let widths: Vec<usize> = (0..=COUNTS.len() + 1).map(width).collect();
    let mut out = String::new();
    for cells in &lines {
        for (column, (cell, &width)) in cells.iter().zip(&widths).enumerate() {
            let _ = match column {
                0 => write!(out, "{cell:<width$}"),
                _ => write!(out, "  {cell:>width$}"),
            };
        }
        out.push('\n');
    }
    let _ = writeln!(
        out,
        "\n{}, at most {} on the stack and {}, {} remembered",
        counted(profile.steps, "step", "steps"),
        counted(profile.peak_stack, "entry", "entries"),
        counted(profile.peak_nodes, "node", "nodes"),
        counted(profile.memo_entries, "result", "results"),
    );
    if profile.restarts > 0 {
        let _ = writeln!(
            out,
            "{} of the parses ran out of their budget of steps and started again with every \
             rule remembered",
            profile.restarts
        );
    }
    for row in rows {
        // What the repeated calls spent, which an average call would overstate where the rules
        // they call are remembered and answer the repeats cheaply.
        let saved = row.repeated_steps;
        if !row.remembered
            && !unmarkable.contains(row.name.as_str())
            && saved * 100 >= profile.steps * SUGGEST_MEMO_PERCENT
        {
            let _ = writeln!(
                out,
                "@memo on {}: {} of its {} calls repeat an earlier one, about {}% of all steps",
                row.name,
                row.repeats,
                row.calls,
                saved * 100 / profile.steps.max(1)
            );
        }
    }
    out
}

/// A count with its noun, which takes the singular for one.
fn counted(count: u64, one: &str, many: &str) -> String {
    match count {
        1 => format!("1 {one}"),
        _ => format!("{count} {many}"),
    }
}

/// The profile as JSON: the totals, and a member `rows` with the counts of each rule reached.
fn as_json(profile: &Profile) -> String {
    let mut out = format!(
        "{{\"steps\":{},\"peak_stack\":{},\"peak_nodes\":{},\"memo_entries\":{},\"restarts\":{},\"rows\":[",
        profile.steps,
        profile.peak_stack,
        profile.peak_nodes,
        profile.memo_entries,
        profile.restarts
    );
    for (index, row) in reached(profile).into_iter().enumerate() {
        out.push_str(if index > 0 {
            ",{\"name\":"
        } else {
            "{\"name\":"
        });
        push_json_string(&mut out, &row.name);
        let _ = write!(out, ",\"remembered\":{}", row.remembered);
        for count in &COUNTS {
            let _ = write!(out, ",\"{}\":{}", count.member, (count.of)(row));
        }
        out.push('}');
    }
    out.push_str("]}\n");
    out
}
