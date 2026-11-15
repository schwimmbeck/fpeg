// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Generated grammars and inputs on the reference interpreter. Resolving and parsing never panic
//! and always end, every grammar prints to text that reads back to the same grammar,
//! memoization never changes a result, a valid input parses the same in both modes, and no parse
//! breaks the item counts of its grammar. `common/generator.rs` makes the grammars.

mod common;

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use common::check_printing;
use common::generator::{check_seeds, generate, reference_limits as limits};
use fpeg::{FieldCount, FieldId, ParseError, ParseErrorKind, Reference, RuleBody, Tree};

/// Resolving and parsing do not panic, and the grammar prints to text that reads back the same.
/// A few grammars are broken on purpose, and `differential.rs` holds how few.
fn check_resolving_parsing_printing(seed: u64) -> Option<String> {
    let (grammar, inputs) = generate(seed);
    // The text front end runs on the reference interpreter too.
    match catch_unwind(|| check_printing(&grammar)) {
        Ok(None) => {}
        Ok(Some(problem)) => return Some(problem),
        Err(_) => return Some(format!("panicked reading back:\n{grammar}")),
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        if let Ok(reference) = Reference::new(&grammar) {
            for input in &inputs {
                let _ = reference.parse_with(input, "start", limits());
                let _ = reference.parse_recovering(input, "start", limits());
            }
        }
    }));
    outcome
        .err()
        .map(|_| format!("panicked on one of {inputs:?}:\n{grammar}"))
}

/// A parse as text, with what it recovered from, or none where it hit a limit.
fn shown(result: Result<Tree, ParseError>, input: &str) -> Option<String> {
    match result {
        Ok(tree) => Some(format!(
            "{}\n{:?}",
            tree.to_text(input, true),
            tree.recoveries()
        )),
        Err(error) if matches!(error.kind, ParseErrorKind::Limit(_)) => None,
        Err(error) => Some(format!("{error}\n{:?}", error.recoveries)),
    }
}

/// Parsing with and without the memo gives the same tree or the same error, in either mode.
fn check_memo(seed: u64) -> Option<String> {
    let (grammar, inputs) = generate(seed);
    let reference = Reference::new(&grammar).ok()?;
    for input in &inputs {
        let pairs = [
            (
                reference.parse_with(input, "start", limits()),
                reference.parse_unmemoized(input, "start", limits()),
            ),
            (
                reference.parse_recovering(input, "start", limits()),
                reference.parse_recovering_unmemoized(input, "start", limits()),
            ),
        ];
        for (memoized, plain) in pairs {
            if let (Some(memoized), Some(plain)) = (shown(memoized, input), shown(plain, input))
                && memoized != plain
            {
                return Some(format!(
                    "on {input:?} the memo gives\n{memoized}\nand no memo gives\n{plain}\n{grammar}"
                ));
            }
        }
    }
    None
}

/// An input that a parse in normal mode accepts gives the same tree in recovering mode, which
/// recovers from nothing.
fn check_modes(seed: u64) -> Option<String> {
    let (grammar, inputs) = generate(seed);
    let reference = Reference::new(&grammar).ok()?;
    for input in &inputs {
        let Ok(normal) = reference.parse_with(input, "start", limits()) else {
            continue;
        };
        let normal = shown(Ok(normal), input);
        let recovering = shown(reference.parse_recovering(input, "start", limits()), input);
        if recovering.is_some() && recovering != normal {
            return Some(format!(
                "on {input:?} normal mode gives\n{normal:?}\nand recovering mode\n{recovering:?}\n\
                 {grammar}"
            ));
        }
    }
    None
}

/// What a parse from `start` shows against the item counts of the grammar: no node holds more
/// items in a field than its kind allows, nor a field its kind lacks, nor lacks a field of
/// exactly one item, no item is of a kind said never to be made, and a start rule said to leave
/// one visible item leaves one.
fn item_count_problem(
    reference: &Reference,
    start: &str,
    input: &str,
    recovering: bool,
) -> Option<String> {
    let parsed = match recovering {
        false => reference.parse_with(input, start, limits()),
        true => reference.parse_recovering(input, start, limits()),
    };
    let Ok(tree) = parsed else {
        return None;
    };
    if reference.single_item(start) == Some(true) {
        let visible = tree.roots().iter().filter(|root| !root.is_hidden()).count();
        if visible != 1 {
            return Some(format!(
                "from {start} on {input:?}: {visible} visible items at the top"
            ));
        }
    }
    let kinds = reference.kinds();
    let mut pending = tree.roots();
    while let Some(item) = pending.pop() {
        if !kinds[item.kind().0 as usize].made {
            return Some(format!(
                "from {start} on {input:?}: an item {} of a kind said never to be made",
                item.kind_name()
            ));
        }
        if item.is_token() {
            continue;
        }
        let children = item.children();
        let mut counts: HashMap<FieldId, (usize, &str)> = HashMap::new();
        for child in children
            .iter()
            .filter(|child| child.field() != FieldId::NONE)
        {
            let name = child.field_name().unwrap_or_default();
            counts.entry(child.field()).or_insert((0, name)).0 += 1;
        }
        let allowed = &kinds[item.kind().0 as usize].fields;
        for &(field, count) in allowed {
            if count == FieldCount::One && !counts.contains_key(&field) {
                return Some(format!(
                    "from {start} on {input:?}: a node {} lacks its field {}, which holds one \
                     item",
                    item.kind_name(),
                    field.0
                ));
            }
        }
        for (field, (count, name)) in counts {
            match allowed.iter().find(|&&(f, _)| f == field) {
                None => {
                    return Some(format!(
                        "from {start} on {input:?}: a node {} holds the field {name}, which its \
                         kind does not have",
                        item.kind_name()
                    ));
                }
                Some((_, FieldCount::One | FieldCount::Optional)) if count > 1 => {
                    return Some(format!(
                        "from {start} on {input:?}: a node {} holds {count} items in the field \
                         {name}, which holds one",
                        item.kind_name()
                    ));
                }
                Some(_) => {}
            }
        }
        pending.extend(children);
    }
    None
}

/// The item counts hold on every parse: from the start rule on every input, and from every other
/// rule and level on the first input, in recovering mode too where the grammar recovers.
fn check_item_counts(seed: u64) -> Option<String> {
    let (grammar, inputs) = generate(seed);
    let reference = Reference::new(&grammar).ok()?;
    let mut starts = Vec::new();
    for rule in grammar.rules.iter().filter(|rule| rule.params.is_empty()) {
        starts.push(rule.name.text.clone());
        if let RuleBody::Precedence(levels) = &rule.body {
            starts.extend(levels.iter().map(|level| level.name.text.clone()));
        }
    }
    let modes: &[bool] = match grammar.recovers.is_empty() {
        true => &[false],
        false => &[false, true],
    };
    for (k, input) in inputs.iter().enumerate() {
        for start in &starts {
            if start != "start" && k > 0 {
                continue;
            }
            for &recovering in modes {
                if let Some(problem) = item_count_problem(&reference, start, input, recovering) {
                    let mode = if recovering {
                        " in recovering mode"
                    } else {
                        ""
                    };
                    return Some(format!("{problem}{mode}\n{grammar}"));
                }
            }
        }
    }
    None
}

#[test]
fn generated_grammars_resolve_parse_and_print_without_failing() {
    check_seeds(check_resolving_parsing_printing);
}

#[test]
fn parses_hold_the_item_counts_of_their_grammar() {
    check_seeds(check_item_counts);
}

#[test]
fn memoization_never_changes_a_result() {
    check_seeds(check_memo);
}

#[test]
fn a_valid_input_gives_the_same_tree_in_both_modes() {
    check_seeds(check_modes);
}
