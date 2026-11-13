// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Generated grammars and inputs on the reference interpreter. Resolving and parsing never panic
//! and always end, every grammar prints to text that reads back to the same grammar,
//! memoization never changes a result, and no parse breaks the item counts of its grammar.
//! `common/generator.rs` makes the grammars.

mod common;

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use common::check_printing;
use common::generator::{check_seeds, generate, reference_limits as limits};
use fpeg::{FieldCount, FieldId, ParseError, ParseErrorKind, Reference, RuleBody, Tree};

/// Resolving and parsing do not panic, and the grammar prints to text that reads back the same.
fn check_robust(seed: u64) -> Option<String> {
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
            }
        }
    }));
    outcome
        .err()
        .map(|_| format!("panicked on one of {inputs:?}:\n{grammar}"))
}

/// Parsing with and without the memo gives the same tree or the same error.
fn check_memo(seed: u64) -> Option<String> {
    let (grammar, inputs) = generate(seed);
    let reference = Reference::new(&grammar).ok()?;
    for input in &inputs {
        let shown = |result: Result<Tree, ParseError>| match result {
            Ok(tree) => Some(tree.to_text(input, true)),
            Err(error) if matches!(error.kind, ParseErrorKind::Limit(_)) => None,
            Err(error) => Some(error.to_string()),
        };
        let memoized = shown(reference.parse_with(input, "start", limits()));
        let plain = shown(reference.parse_unmemoized(input, "start", limits()));
        if let (Some(memoized), Some(plain)) = (memoized, plain)
            && memoized != plain
        {
            return Some(format!(
                "on {input:?} the memo gives\n{memoized}\nand no memo gives\n{plain}\n{grammar}"
            ));
        }
    }
    None
}

/// What a parse from `start` shows against the item counts of the grammar: no node holds more
/// items in a field than its kind allows, nor a field its kind lacks, nor lacks a field of
/// exactly one item, no item is of a kind said never to be made, and a start rule said to leave
/// one visible item leaves one.
fn item_count_problem(reference: &Reference, start: &str, input: &str) -> Option<String> {
    let Ok(tree) = reference.parse_with(input, start, limits()) else {
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
/// rule and level on the first input.
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
    for (k, input) in inputs.iter().enumerate() {
        for start in &starts {
            if start != "start" && k > 0 {
                continue;
            }
            if let Some(problem) = item_count_problem(&reference, start, input) {
                return Some(format!("{problem}\n{grammar}"));
            }
        }
    }
    None
}

#[test]
fn generated_grammars_resolve_parse_and_print_without_failing() {
    check_seeds(check_robust);
}

#[test]
fn parses_hold_the_item_counts_of_their_grammar() {
    check_seeds(check_item_counts);
}

#[test]
fn memoization_never_changes_a_result() {
    check_seeds(check_memo);
}
