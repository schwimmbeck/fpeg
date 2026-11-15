// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The grammars and inputs that the generator of the tests makes from the fuzzer's bytes give
//! the parsing machine the trees and errors of the reference interpreter, in recovering mode
//! too where a grammar recovers. The first eight bytes pick what the machine remembers and the
//! passes it runs, and the rest drive the generator's choices, a byte for each, so that every
//! input is a grammar and coverage steers through the engine. Parses where either engine hits
//! a limit are skipped, since the two count steps differently, but for the depth limit, which
//! inputs this short reach only where the machine descends without end.

#![no_main]

#[path = "../../crates/fpeg/tests/common/generator.rs"]
#[allow(
    dead_code,
    reason = "the target uses the generator, not the runner of the tests"
)]
mod generator;

use fpeg::{Limit, Limits, Memo, Options, ParseError, ParseErrorKind, Parser, Reference, Tree};
use libfuzzer_sys::fuzz_target;

/// Steps a parse may take on the machine, whose steps do less than the interpreter's.
const MACHINE_MAX_STEPS: u64 = 200_000;

/// What the machine remembers, one of them for each input.
const MEMO: [Memo; 3] = [Memo::Grammar, Memo::Off, Memo::All];

/// A parse as text: the tree with spans and its recoveries, or the error with its recoveries,
/// or the limit hit.
fn shown(result: Result<Tree, ParseError>, input: &str) -> Result<String, Limit> {
    match result {
        Ok(tree) => Ok(format!(
            "{}\n{:?}",
            tree.to_text(input, true),
            tree.recoveries()
        )),
        Err(ParseError {
            kind: ParseErrorKind::Limit(limit),
            ..
        }) => Err(limit),
        Err(error) => Ok(format!("{error:?}")),
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((setting, choices)) = data.split_first_chunk::<8>() else {
        return;
    };
    let setting = u64::from_le_bytes(*setting);
    let (grammar, inputs) = generator::generate_from(choices);
    let options = Options {
        memo: MEMO[(setting % 3) as usize],
        passes: generator::sampled_passes(setting / 3),
        ..Options::default()
    };
    let engines = (
        Reference::new(&grammar),
        Parser::with_options(&grammar, &options),
    );
    let (reference, parser) = match engines {
        (Ok(reference), Ok(parser)) => (reference, parser),
        (Err(expected), Err(errors)) => {
            assert_eq!(
                expected, errors,
                "the engines reject it differently\n{grammar}"
            );
            return;
        }
        (expected, errors) => panic!(
            "one engine rejects it: {:?} {:?}\n{grammar}",
            expected.err(),
            errors.err()
        ),
    };
    let machine = Limits {
        max_steps: MACHINE_MAX_STEPS,
        ..Limits::default()
    };
    let modes: &[bool] = match grammar.recovers.is_empty() {
        true => &[false],
        false => &[false, true],
    };
    for input in &inputs {
        for &recovering in modes {
            let (expected, actual) = match recovering {
                true => (
                    reference.parse_recovering(input, "start", generator::reference_limits()),
                    parser.parse_recovering_with(input, "start", machine),
                ),
                false => (
                    reference.parse_with(input, "start", generator::reference_limits()),
                    parser.parse_with(input, "start", machine),
                ),
            };
            match (shown(expected, input), shown(actual, input)) {
                (Ok(expected), Ok(actual)) => assert_eq!(
                    expected, actual,
                    "on {input:?} with {options:?}, recovering {recovering}\n{grammar}"
                ),
                (Ok(_), Err(Limit::Depth)) => {
                    panic!("on {input:?} only the machine hits the depth limit\n{grammar}")
                }
                _ => {}
            }
        }
    }
});
