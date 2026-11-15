// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Modules: a grammar joined with the grammars it extends into one grammar, before it resolves.
//!
//! The text of each module is read with its spans placed after those of the grammars before it,
//! so that a problem the resolver or `check` finds in a module's rules can be traced back to it.

use std::borrow::Cow;
use std::collections::HashMap;

use super::{Grammar, Span, Text};
use crate::diag::{Diagnostic, DiagnosticCode};
use crate::lines::LineIndex;

/// The text of a grammar that comes with the library, by the name a grammar extends it by.
pub fn bundled(name: &str) -> Option<&'static str> {
    match name {
        "json" => Some(include_str!("../../grammars/json.fpeg")),
        "python" => Some(include_str!("../../grammars/python.fpeg")),
        _ => None,
    }
}

/// The text of a module, where its spans start, and the directive of the grammar that took it
/// in, directly or through other modules.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Source {
    name: String,
    text: &'static str,
    base: u32,
    directive: Span,
}

impl Grammar {
    /// The diagnostics, each whose span lies in a module joined in moved to the directive that
    /// took the module in, with its message saying where in the module.
    pub(crate) fn locate(&self, diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
        diagnostics
            .into_iter()
            .map(|d| locate(&self.sources, d))
            .collect()
    }
}

fn locate(sources: &[Source], diagnostic: Diagnostic) -> Diagnostic {
    let at = diagnostic.span.start;
    let Some(source) =
        (sources.iter()).find(|s| s.base <= at && (at - s.base) as usize <= s.text.len())
    else {
        return diagnostic;
    };
    let lines = LineIndex::new(source.text);
    let (line, column) = lines.line_column(source.text, (at - source.base) as usize);
    Diagnostic {
        message: format!(
            "in the grammar {}, line {line}, column {column}: {}",
            source.name, diagnostic.message
        ),
        span: source.directive,
        ..diagnostic
    }
}

/// Join a grammar with the grammars it extends.
pub(crate) fn link(grammar: &Grammar) -> Result<Cow<'_, Grammar>, Vec<Diagnostic>> {
    if grammar.extends.is_none() {
        return Ok(Cow::Borrowed(grammar));
    }
    let mut linker = Linker {
        sources: Vec::new(),
        next_base: extent(grammar) + 1,
        directive: None,
    };
    match linker.extended(grammar.clone()) {
        Ok(mut joined) => {
            joined.sources = linker.sources;
            Ok(Cow::Owned(joined))
        }
        Err(errors) => Err(errors
            .into_iter()
            .map(|d| locate(&linker.sources, d))
            .collect()),
    }
}

/// The end of the last span of a grammar, past which the texts of its modules can lie.
fn extent(grammar: &Grammar) -> u32 {
    let rules = grammar.rules.iter().map(|r| r.span.end);
    let ignore = grammar.ignore.iter().map(|e| e.span.end);
    let word = grammar.word.iter().map(|e| e.span.end);
    let flags = grammar.flags.iter().map(|f| f.span.end);
    let tabs = grammar.tabs.iter().map(|t| t.span.end);
    let words = grammar.soft_keywords.iter().map(|w| w.span.end);
    let imports = grammar.imports.iter().map(|i| i.module.span.end);
    let extends = grammar.extends.iter().map(|e| e.span.end);
    let recovers = grammar.recovers.iter().map(|r| r.expr.span.end);
    (rules
        .chain(ignore)
        .chain(word)
        .chain(flags)
        .chain(tabs)
        .chain(words))
    .chain(imports)
    .chain(extends)
    .chain(recovers)
    .max()
    .unwrap_or(0)
}

struct Linker {
    sources: Vec<Source>,
    next_base: u32,
    /// The directive of the grammar itself that the modules being read come through.
    directive: Option<Span>,
}

fn problem(code: DiagnosticCode, message: String, span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic {
        code,
        message,
        span,
    }]
}

impl Linker {
    /// The grammar joined with what it extends, through any number of grammars.
    fn extended(&mut self, mut grammar: Grammar) -> Result<Grammar, Vec<Diagnostic>> {
        let Some(base) = grammar.extends.take() else {
            return Ok(grammar);
        };
        let module = self.read(&base)?;
        let module = self.extended(module)?;
        Ok(extend(module, grammar))
    }

    /// The grammar model of a module.
    fn read(&mut self, name: &Text) -> Result<Grammar, Vec<Diagnostic>> {
        let Some(text) = bundled(&name.value) else {
            let message = format!("no grammar is named {}", name.value);
            return Err(problem(DiagnosticCode::UnknownModule, message, name.span));
        };
        let directive = *self.directive.get_or_insert(name.span);
        let base = self.next_base;
        let too_large = || {
            let message = "the grammars extended take too much text".to_owned();
            problem(DiagnosticCode::TooLarge, message, directive)
        };
        let length = u32::try_from(text.len()).map_err(|_| too_large())?;
        self.next_base = (base.checked_add(length))
            .and_then(|end| end.checked_add(1))
            .ok_or_else(too_large)?;
        self.sources.push(Source {
            name: name.value.clone(),
            text,
            base,
            directive,
        });
        Grammar::read_module(text, base)
    }
}

/// The base with the rules and directives of the extension: a rule defined again replaces the
/// inherited one, and the extension's directives follow or replace the base's.
fn extend(mut base: Grammar, extension: Grammar) -> Grammar {
    let mut index: HashMap<String, usize> = (base.rules.iter().enumerate())
        .map(|(i, rule)| (rule.name.text.clone(), i))
        .collect();
    for rule in extension.rules {
        // A name replaces once, so that a second definition stays and is reported as one.
        match index.remove(&rule.name.text) {
            Some(i) => base.rules[i] = rule,
            None => base.rules.push(rule),
        }
    }
    base.ignore.extend(extension.ignore);
    if extension.word.is_some() {
        base.word = extension.word;
    }
    for flag in extension.flags {
        if !base.flags.iter().any(|f| f.text == flag.text) {
            base.flags.push(flag);
        }
    }
    if extension.tabs.is_some() {
        base.tabs = extension.tabs;
    }
    for word in extension.soft_keywords {
        if !base.soft_keywords.iter().any(|w| w.value == word.value) {
            base.soft_keywords.push(word);
        }
    }
    base.imports.extend(extension.imports);
    base.recovers.extend(extension.recovers);
    base
}
