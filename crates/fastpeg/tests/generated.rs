// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! Generated grammars and inputs. Resolving and parsing never panic and always end, every
//! grammar prints to text that reads back to the same grammar, and memoization never changes a
//! result.
//!
//! `FASTPEG_GENERATED_CASES` sets how many grammars to try and `FASTPEG_GENERATED_SEED` the first
//! seed, for longer runs than the default and to replay a reported failure.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;

use fastpeg::grammar::builder::*;
use fastpeg::grammar::{ClassItem, Expr, ExprKind, Grammar, Level, RuleBody, Span, Text};
use fastpeg::reference::{Limits, ParseError, ParseErrorKind, Reference};
use fastpeg::tree::Tree;

/// Grammars a normal test run tries.
const DEFAULT_CASES: u64 = 400;

/// Inputs tried on each grammar.
const INPUTS_PER_GRAMMAR: usize = 8;

/// The longest random input, in characters.
const MAX_INPUT_CHARS: u64 = 10;

/// The longest input derived from a grammar, in characters.
const MAX_DERIVED_CHARS: usize = 40;

/// How deep the derivation of an input follows rules and expressions.
const MAX_DERIVE_DEPTH: u32 = 12;

/// The deepest generated expression.
const MAX_DEPTH: u32 = 4;

/// Steps a parse may take, so that grammars with exponential time end quickly.
const MAX_STEPS: u64 = 20_000;

/// The characters inputs are made of: the letters the grammars use, layout, and a two-byte one.
const INPUT_CHARS: [char; 6] = ['a', 'b', 'c', ' ', '\n', 'é'];

/// SplitMix64, enough to spread seeds over the choices below.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn pick<'a>(&mut self, items: &'a [String]) -> &'a str {
        &items[self.below(items.len() as u64) as usize]
    }
}

/// A sequence or choice of the given parts, flattened the way the text front end reads them.
fn flat(parts: Vec<Expr>, choice: bool) -> Expr {
    let mut items = Vec::new();
    for part in parts {
        match part.kind {
            ExprKind::Sequence(inner) if !choice => items.extend(inner),
            ExprKind::Choice(inner) if choice => items.extend(inner),
            kind => items.push(Expr::new(kind)),
        }
    }
    match (items.len(), choice) {
        (1, _) => items.remove(0),
        (_, false) => Expr::new(ExprKind::Sequence(items)),
        (_, true) => Expr::new(ExprKind::Choice(items)),
    }
}

/// What a reference means where an input is derived: template arguments and precedence levels.
#[derive(Default)]
struct Scope<'g> {
    params: Vec<(&'g str, &'g Expr)>,
    levels: Option<&'g [Level]>,
}

struct Generator {
    rng: Rng,
    rules: Vec<String>,
    terminals: Vec<String>,
    /// Whether the grammar has the template `t<p>`.
    template: bool,
    /// The parameters and level names the expression being generated may use.
    params: Vec<String>,
    levels: Vec<String>,
    /// The texts the derivation of the current input captured, latest last.
    captured: Vec<(String, String)>,
}

impl Generator {
    fn terminal_or_literal(&mut self) -> Expr {
        match self.terminals.is_empty() {
            true => lit("c"),
            false => r(self.rng.pick(&self.terminals)),
        }
    }

    fn atom(&mut self, terminal: bool) -> Expr {
        match self.rng.below(13) {
            0 => lit("a"),
            1 => lit("b"),
            2 => lit("ab"),
            3 => class([range('a', 'b')]),
            4 => not_class([ch('a'), ch('\n')]),
            5 => any(),
            6 => empty(),
            7 => lit_i("ab"),
            8 => capture_ref("c"),
            9 if !self.params.is_empty() => r(self.rng.pick(&self.params)),
            10 if !self.levels.is_empty() => r(self.rng.pick(&self.levels)),
            // Closed arguments keep the number of template instances small.
            11 if self.template && !terminal => {
                let argument = match self.rng.chance(50) {
                    true => class([range('a', 'b')]),
                    false => self.terminal_or_literal(),
                };
                r_args("t", [argument])
            }
            _ if terminal || self.rng.chance(30) => self.terminal_or_literal(),
            _ => r(self.rng.pick(&self.rules)),
        }
    }

    fn parts(&mut self, depth: u32, terminal: bool) -> Vec<Expr> {
        let count = 2 + self.rng.below(2);
        (0..count).map(|_| self.expr(depth + 1, terminal)).collect()
    }

    fn expr(&mut self, depth: u32, terminal: bool) -> Expr {
        if depth >= MAX_DEPTH || self.rng.chance(35) {
            return self.atom(terminal);
        }
        let kinds = if terminal { 8 } else { 14 };
        match self.rng.below(kinds) {
            0 | 1 => flat(self.parts(depth, terminal), false),
            2 | 3 => flat(self.parts(depth, terminal), true),
            4 => {
                let inner = self.expr(depth + 1, terminal);
                match self.rng.below(4) {
                    0 => opt(inner),
                    1 => many(inner),
                    2 => some(inner),
                    _ => {
                        let min = self.rng.below(3) as u32;
                        rep(inner, min, Some(min + self.rng.below(2) as u32))
                    }
                }
            }
            5 => {
                let item = self.expr(depth + 1, terminal);
                let separator = self.atom(terminal);
                match self.rng.chance(50) {
                    true => sep_many(item, separator),
                    false => sep_some(item, separator),
                }
            }
            6 => match self.rng.chance(50) {
                true => and(self.expr(depth + 1, terminal)),
                false => not(self.expr(depth + 1, terminal)),
            },
            7 => def("c", self.expr(depth + 1, terminal)),
            8 => label_message(self.expr(depth + 1, false), "oops"),
            9 | 10 => {
                let name = if self.rng.chance(50) { "x" } else { "y" };
                field(name, self.expr(depth + 1, false))
            }
            11 => {
                let (first, second) = (self.expr(depth + 1, false), self.expr(depth + 1, false));
                flat(vec![first, cut(), second], false)
            }
            12 => match self.rng.below(3) {
                0 => on("f", self.expr(depth + 1, false)),
                1 => off("f", self.expr(depth + 1, false)),
                _ => flat(vec![if_flag("f"), self.expr(depth + 1, false)], false),
            },
            _ => match self.rng.chance(50) {
                true => indent(self.expr(depth + 1, false)),
                false => flat(vec![same(), self.expr(depth + 1, false)], false),
            },
        }
    }

    fn alternatives(&mut self) -> Vec<fastpeg::grammar::Alternative> {
        (0..1 + self.rng.below(3))
            .map(|_| {
                let expr = self.expr(0, false);
                match self.rng.chance(20) {
                    true => alt(expr).alias("node"),
                    false => alt(expr),
                }
            })
            .collect()
    }

    /// The levels of the precedence block `prec`: operators of each kind, and at the bottom
    /// operands and a bracketed `prec`.
    fn levels(&mut self) -> Vec<Level> {
        let names: Vec<String> = (1..=2 + self.rng.below(2))
            .map(|i| format!("l{i}"))
            .collect();
        self.levels = names.clone();
        let mut levels = Vec::new();
        for (k, name) in names.iter().enumerate() {
            let mut alternatives = Vec::new();
            match names.get(k + 1) {
                Some(next) => {
                    for _ in 0..1 + self.rng.below(2) {
                        let left = seq([field("left", r(name)), lit("a"), field("right", r(next))]);
                        let right =
                            seq([field("left", r(next)), lit("b"), field("right", r(name))]);
                        alternatives.push(match self.rng.below(4) {
                            0 => alt(left).alias("binary"),
                            1 => alt(right).alias("binary"),
                            2 => alt(seq([lit("b"), field("operand", r(name))])).alias("unary"),
                            _ => alt(self.expr(1, false)).alias("other"),
                        });
                    }
                }
                None => {
                    alternatives.push(alt(self.atom(false)));
                    alternatives.push(alt(seq([lit("b"), r("prec"), lit("a")])));
                }
            }
            levels.push(level(name, alternatives));
        }
        self.levels.clear();
        levels
    }

    fn grammar(&mut self) -> Grammar {
        let mut g = GrammarBuilder::new();
        g.flag("f");
        if self.rng.chance(50) {
            g.ignore(some(class([ch(' ')])));
        }
        let word = self.rng.chance(30);
        if word {
            g.word(class([range('a', 'z')]));
        }
        for name in self.rules.clone() {
            if name == "prec" {
                let levels = self.levels();
                g.precedence(&name, levels);
                continue;
            }
            let alternatives = self.alternatives();
            let rule = g.rule_alternatives(&name, alternatives);
            if !name.starts_with('_') && name != "start" && self.rng.chance(20) {
                rule.transparent();
            }
            if self.rng.chance(10) {
                rule.attribute("lexical");
            }
        }
        if self.template {
            self.params = vec!["p".to_owned()];
            let body = self.expr(0, false);
            self.params.clear();
            g.rule("t", body).params(["p"]);
        }
        for name in self.terminals.clone() {
            let body = self.expr(0, true);
            let rule = g.rule(&name, body);
            if self.rng.chance(20) {
                rule.display("thing");
            }
            if self.rng.chance(25) {
                rule.attribute("reserved");
            }
        }
        let mut grammar = g.build();
        if word && self.rng.chance(50) {
            grammar.soft_keywords.push(Text {
                value: "ab".to_owned(),
                span: Span::default(),
            });
        }
        grammar
    }

    fn char(&mut self) -> char {
        INPUT_CHARS[self.rng.below(INPUT_CHARS.len() as u64) as usize]
    }

    /// An input for the grammar: most are derived from it, so that parses get somewhere, some
    /// of those get one character changed, and the rest are random.
    fn input(&mut self, grammar: &Grammar, index: usize) -> String {
        let mut text = String::new();
        match index % 4 {
            0 => {
                for _ in 0..self.rng.below(MAX_INPUT_CHARS + 1) {
                    text.push(self.char());
                }
            }
            choice => {
                self.captured.clear();
                let start = r("start");
                self.derive(grammar, &start, &Scope::default(), 0, &mut text);
                let mut chars: Vec<char> = text.chars().take(MAX_DERIVED_CHARS).collect();
                if choice == 1 {
                    let at = self.rng.below(chars.len() as u64 + 1) as usize;
                    match chars.len() > at && self.rng.chance(50) {
                        true => drop(chars.remove(at)),
                        false => chars.insert(at, self.char()),
                    }
                }
                text = chars.into_iter().collect();
            }
        }
        text
    }

    /// Follow a precedence block from level `k`, falling through to tighter levels at random.
    fn derive_level<'g>(
        &mut self,
        grammar: &'g Grammar,
        levels: &'g [Level],
        mut k: usize,
        depth: u32,
        out: &mut String,
    ) {
        while k + 1 < levels.len() && self.rng.chance(40) {
            k += 1;
        }
        let alternatives = &levels[k].alternatives;
        let pick = self.rng.below(alternatives.len() as u64) as usize;
        let scope = Scope {
            params: Vec::new(),
            levels: Some(levels),
        };
        self.derive(grammar, &alternatives[pick].expr, &scope, depth, out);
    }

    /// Append text that the expression likely matches, ignoring lookahead and context.
    fn derive<'g>(
        &mut self,
        grammar: &'g Grammar,
        expr: &'g Expr,
        scope: &Scope<'g>,
        depth: u32,
        out: &mut String,
    ) {
        if depth > MAX_DERIVE_DEPTH || out.len() > MAX_DERIVED_CHARS {
            return;
        }
        match &expr.kind {
            ExprKind::Literal {
                text,
                case_insensitive,
            } => {
                for c in text.chars() {
                    match *case_insensitive && self.rng.chance(50) {
                        true => out.push(c.to_ascii_uppercase()),
                        false => out.push(c),
                    }
                }
            }
            ExprKind::Class(class) => {
                let fits: Vec<char> = INPUT_CHARS
                    .into_iter()
                    .filter(|&c| class.items.iter().any(|i| contains(i, c)) != class.negated)
                    .collect();
                if !fits.is_empty() {
                    out.push(fits[self.rng.below(fits.len() as u64) as usize]);
                }
            }
            ExprKind::Any => out.push(self.char()),
            ExprKind::Reference { name, args } => {
                let rule = grammar.rules.iter().find(|r| r.name.text == name.text);
                let level = scope
                    .levels
                    .and_then(|levels| levels.iter().position(|l| l.name.text == name.text));
                if let Some(&(_, argument)) = scope.params.iter().find(|(p, _)| *p == name.text) {
                    self.derive(grammar, argument, &Scope::default(), depth + 1, out);
                } else if let (Some(levels), Some(k)) = (scope.levels, level) {
                    self.derive_level(grammar, levels, k, depth + 1, out);
                } else if let Some(rule) = rule {
                    match &rule.body {
                        RuleBody::Alternatives(alternatives) => {
                            let pick = self.rng.below(alternatives.len() as u64) as usize;
                            let params = rule.params.iter().map(|p| p.text.as_str());
                            let inner = Scope {
                                params: params.zip(args.iter()).collect(),
                                levels: None,
                            };
                            self.derive(grammar, &alternatives[pick].expr, &inner, depth + 1, out);
                        }
                        RuleBody::Precedence(levels) => {
                            self.derive_level(grammar, levels, 0, depth + 1, out);
                        }
                    }
                }
            }
            ExprKind::Sequence(items) => {
                for item in items {
                    if !grammar.ignore.is_empty() && self.rng.chance(20) {
                        out.push(' ');
                    }
                    self.derive(grammar, item, scope, depth + 1, out);
                }
            }
            ExprKind::Choice(items) => {
                let pick = self.rng.below(items.len() as u64) as usize;
                self.derive(grammar, &items[pick], scope, depth + 1, out);
            }
            ExprKind::Repeat { expr, min, max } => {
                let extra = max.map_or(2, |max| max.saturating_sub(*min).min(2));
                for _ in 0..*min as u64 + self.rng.below(u64::from(extra) + 1) {
                    self.derive(grammar, expr, scope, depth + 1, out);
                }
            }
            ExprKind::Separated {
                item,
                separator,
                at_least_one,
            } => {
                let count = u64::from(*at_least_one) + self.rng.below(3);
                for i in 0..count {
                    if i > 0 {
                        self.derive(grammar, separator, scope, depth + 1, out);
                    }
                    self.derive(grammar, item, scope, depth + 1, out);
                }
            }
            ExprKind::Def { name, expr } => {
                let start = out.len();
                self.derive(grammar, expr, scope, depth + 1, out);
                self.captured
                    .push((name.text.clone(), out[start..].to_owned()));
            }
            ExprKind::Ref { name } => {
                if let Some((_, text)) = self.captured.iter().rev().find(|(n, _)| *n == name.text) {
                    out.push_str(&text.clone());
                }
            }
            ExprKind::Field { expr, .. }
            | ExprKind::Label { expr, .. }
            | ExprKind::FlagOn { expr, .. }
            | ExprKind::FlagOff { expr, .. }
            | ExprKind::Indent(expr) => self.derive(grammar, expr, scope, depth + 1, out),
            _ => {}
        }
    }
}

fn contains(item: &ClassItem, c: char) -> bool {
    match item {
        ClassItem::Range(first, last) => (*first..=*last).contains(&c),
        ClassItem::Property { .. } => false,
    }
}

fn shape(grammar: &Grammar) -> String {
    let debug = format!("{grammar:?}");
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug.as_str();
    while let Some(at) = rest.find("Span { start: ") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let end = after.find(" }").map_or(after.len(), |e| e + 2);
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn env_number(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// The grammar and inputs of one seed.
fn generate(seed: u64) -> (Grammar, Vec<String>) {
    let mut rng = Rng(seed);
    let mut rules: Vec<String> = (0..1 + rng.below(4))
        .map(|i| match (i, rng.below(6)) {
            (0, _) => "start".to_owned(),
            (_, 0) => format!("_r{i}"),
            _ => format!("r{i}"),
        })
        .collect();
    if rng.chance(25) {
        rules.push("prec".to_owned());
    }
    let template = rng.chance(25);
    let terminals = (0..rng.below(3))
        .map(|i| match rng.chance(25) {
            true => format!("_T{i}"),
            false => format!("T{i}"),
        })
        .collect();
    let mut generator = Generator {
        rng,
        rules,
        terminals,
        template,
        params: Vec::new(),
        levels: Vec::new(),
        captured: Vec::new(),
    };
    let grammar = generator.grammar();
    let inputs = (0..INPUTS_PER_GRAMMAR)
        .map(|i| generator.input(&grammar, i))
        .collect();
    (grammar, inputs)
}

fn limits() -> Limits {
    Limits {
        max_steps: MAX_STEPS,
        ..Limits::default()
    }
}

/// Resolving and parsing do not panic, and the grammar prints to text that reads back the same.
fn check_robust(seed: u64) -> Option<String> {
    let (grammar, inputs) = generate(seed);
    let printed = grammar.to_string();
    // The text front end runs on the reference interpreter too.
    match catch_unwind(|| Grammar::from_text(&printed)) {
        Ok(Ok(again)) if shape(&again) == shape(&grammar) => {}
        Ok(Ok(_)) => return Some(format!("reads back differently:\n{printed}")),
        Ok(Err(errors)) => return Some(format!("does not read back: {errors:?}\n{printed}")),
        Err(_) => return Some(format!("panicked reading back:\n{printed}")),
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
        .map(|_| format!("panicked on one of {inputs:?}:\n{printed}"))
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

/// Run a check on every seed, spread over the available threads, and fail with the first few
/// problems.
fn check_seeds(check: fn(u64) -> Option<String>) {
    let cases = env_number("FASTPEG_GENERATED_CASES", DEFAULT_CASES);
    let first = env_number("FASTPEG_GENERATED_SEED", 0);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()) as u64;
    let failures = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for offset in 0..threads.min(cases) {
            let failures = &failures;
            scope.spawn(move || {
                for seed in (first + offset..first + cases).step_by(threads as usize) {
                    let outcome = catch_unwind(|| check(seed));
                    let problem = outcome.unwrap_or_else(|_| Some("panicked".to_owned()));
                    if let Some(problem) = problem {
                        let mut failures = failures.lock().expect("no thread panics holding it");
                        failures.push(format!("seed {seed}: {problem}"));
                    }
                }
            });
        }
    });
    let mut failures = failures
        .into_inner()
        .expect("no thread panicked holding it");
    failures.sort();
    failures.truncate(5);
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn generated_grammars_resolve_parse_and_print_without_failing() {
    check_seeds(check_robust);
}

#[test]
fn memoization_never_changes_a_result() {
    check_seeds(check_memo);
}
