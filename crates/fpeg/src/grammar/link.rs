// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Modules: a grammar joined with the grammars it extends and imports into one grammar, before
//! it resolves.
//!
//! The text of each module is read with its spans placed after those of the grammars before it,
//! so that a problem the resolver or `check` finds in a module's rules can be traced back to it.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use super::{
    Alternative, Expr, ExprKind, Grammar, Import, Level, Name, Rule, RuleBody, Span, Text,
};
use crate::diag::{Diagnostic, DiagnosticCode};
use crate::lines::LineIndex;

/// The text of a grammar that comes with the library, by the name a grammar extends or imports
/// it by.
pub fn bundled(name: &str) -> Option<&'static str> {
    match name {
        "common" => Some(include_str!("../../grammars/common.fpeg")),
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

/// Join a grammar with the grammars it extends and imports.
pub(crate) fn link(grammar: &Grammar) -> Result<Cow<'_, Grammar>, Vec<Diagnostic>> {
    if grammar.extends.is_none() && grammar.imports.is_empty() {
        return Ok(Cow::Borrowed(grammar));
    }
    let mut linker = Linker {
        sources: Vec::new(),
        next_base: extent(grammar) + 1,
    };
    match linker.linked(grammar.clone(), None) {
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
    // The names and the prefix of an import stand after its module, and errors point at them.
    let imports = grammar.imports.iter().flat_map(|i| {
        let names = i.names.iter().map(|n| n.span.end);
        names
            .chain(i.prefix.iter().map(|p| p.span.end))
            .chain([i.module.span.end])
    });
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
}

fn problem(code: DiagnosticCode, message: String, span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic {
        code,
        message,
        span,
    }]
}

impl Linker {
    /// A grammar joined with what it extends and imports, through any number of grammars.
    /// `directive` is the one of the grammar itself that this grammar came in through.
    fn linked(
        &mut self,
        mut grammar: Grammar,
        directive: Option<Span>,
    ) -> Result<Grammar, Vec<Diagnostic>> {
        if let Some(base) = grammar.extends.take() {
            let through = directive.unwrap_or(base.span);
            let module = self.read(&base, through)?;
            let module = self.linked(module, Some(through))?;
            grammar = extend(module, grammar)?;
        }
        for taken in by_module(std::mem::take(&mut grammar.imports))? {
            let through = directive.unwrap_or(taken.module.span);
            let module = self.read(&taken.module, through)?;
            let module = self.linked(module, Some(through))?;
            import(&mut grammar, module, &taken.module, taken.names)?;
        }
        Ok(grammar)
    }

    /// The grammar model of a module.
    fn read(&mut self, name: &Text, directive: Span) -> Result<Grammar, Vec<Diagnostic>> {
        let Some(text) = bundled(&name.value) else {
            let message = format!("no grammar is named {}", name.value);
            return Err(problem(DiagnosticCode::UnknownModule, message, name.span));
        };
        let base = self.next_base;
        let too_large = || {
            let message = "the grammars extended and imported take too much text".to_owned();
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
fn extend(mut base: Grammar, extension: Grammar) -> Result<Grammar, Vec<Diagnostic>> {
    let mut index: HashMap<String, usize> = (base.rules.iter().enumerate())
        .map(|(i, rule)| (rule.name.text.clone(), i))
        .collect();
    let mut errors = Vec::new();
    for rule in extension.rules {
        // A name replaces once, so that a second definition stays and is reported as one.
        match index.remove(&rule.name.text) {
            Some(i) => match splice(rule, &base.rules[i]) {
                Ok(replaced) => base.rules[i] = replaced,
                Err(error) => errors.extend(error),
            },
            None => base.rules.push(rule),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    base.ignore.extend(extension.ignore);
    if extension.word.is_some() {
        base.word = extension.word;
    }
    join_flags(&mut base.flags, extension.flags);
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
    Ok(base)
}

/// The flags of a grammar with those of another added, each name once.
fn join_flags(flags: &mut Vec<Name>, more: Vec<Name>) {
    for flag in more {
        if !flags.iter().any(|f| f.text == flag.text) {
            flags.push(flag);
        }
    }
}

/// The spans of every `...` in an expression.
fn inherited(expr: &Expr, found: &mut Vec<Span>) {
    let mut pending = vec![expr];
    while let Some(expr) = pending.pop() {
        if matches!(expr.kind, ExprKind::Inherited) {
            found.push(expr.span);
        }
        pending.extend(expr.children());
    }
}

/// A rule that replaces an inherited one, with its `...` taking the place of the inherited
/// alternatives.
fn splice(mut rule: Rule, old: &Rule) -> Result<Rule, Vec<Diagnostic>> {
    let misplaced = |span, message: String| problem(DiagnosticCode::Inherited, message, span);
    let not_yet = |span, form: &str| {
        let message = format!("... {form} is not supported yet, see the roadmap (M9)");
        problem(DiagnosticCode::NotYet, message, span)
    };
    let alternatives = match &rule.body {
        RuleBody::Alternatives(alternatives) => alternatives,
        RuleBody::Precedence(levels) => {
            let mut found = Vec::new();
            for level in levels {
                level
                    .alternatives
                    .iter()
                    .for_each(|a| inherited(&a.expr, &mut found));
            }
            return match found.first() {
                Some(&span) => Err(not_yet(span, "in a precedence block")),
                None => Ok(rule),
            };
        }
    };
    let mut nested = Vec::new();
    for alternative in alternatives {
        alternative
            .expr
            .children()
            .into_iter()
            .for_each(|e| inherited(e, &mut nested));
    }
    if let Some(&span) = nested.first() {
        let message = "... stands only as a whole alternative".to_owned();
        return Err(misplaced(span, message));
    }
    let whole: Vec<usize> = (alternatives.iter().enumerate())
        .filter(|(_, a)| matches!(a.expr.kind, ExprKind::Inherited))
        .map(|(i, _)| i)
        .collect();
    let Some(&at) = whole.first() else {
        return Ok(rule);
    };
    if let Some(&again) = whole.get(1) {
        let message = "... stands at most once in a rule".to_owned();
        return Err(misplaced(alternatives[again].expr.span, message));
    }
    let span = alternatives[at].expr.span;
    if let Some(alias) = &alternatives[at].alias {
        let message = "... keeps the aliases of the inherited alternatives and takes none";
        return Err(misplaced(alias.span, message.to_owned()));
    }
    let RuleBody::Alternatives(inherited) = &old.body else {
        return Err(not_yet(span, "for a precedence block"));
    };
    let same = rule.params.len() == old.params.len()
        && (rule.params.iter().zip(&old.params)).all(|(new, old)| new.text == old.text);
    if !same {
        let message = format!(
            "{} takes other parameters than the rule it replaces, whose alternatives ... stands for",
            rule.name.text
        );
        return Err(misplaced(span, message));
    }
    if let RuleBody::Alternatives(alternatives) = &mut rule.body {
        alternatives.splice(at..=at, inherited.iter().cloned());
    }
    Ok(rule)
}

/// What a grammar imports from one module: the rules it names, or every rule.
struct Taken {
    module: Text,
    /// `None` where an import names no rules and so takes them all.
    names: Option<Vec<Name>>,
}

/// The imports of a grammar by module, in the order they first name it.
fn by_module(imports: Vec<Import>) -> Result<Vec<Taken>, Vec<Diagnostic>> {
    let mut modules: Vec<Taken> = Vec::new();
    for import in imports {
        if let Some(prefix) = &import.prefix {
            let message = "an import under a prefix is not supported yet, see the roadmap (M9)";
            return Err(problem(DiagnosticCode::NotYet, message.into(), prefix.span));
        }
        let names = (!import.names.is_empty()).then_some(import.names);
        match modules
            .iter_mut()
            .find(|t| t.module.value == import.module.value)
        {
            Some(taken) => match (taken.names.as_mut(), names) {
                (Some(taken), Some(more)) => taken.extend(more),
                _ => taken.names = None,
            },
            None => modules.push(Taken {
                module: import.module,
                names,
            }),
        }
    }
    Ok(modules)
}

/// The names of the levels of a rule, none unless its body is a precedence block.
fn levels(rule: &Rule) -> impl Iterator<Item = &str> {
    let levels = match &rule.body {
        RuleBody::Alternatives(_) => &[][..],
        RuleBody::Precedence(levels) => levels.as_slice(),
    };
    levels.iter().map(|level| level.name.text.as_str())
}

/// The names a rule refers to, rules and levels, its parameters left out.
fn references(rule: &Rule) -> Vec<&str> {
    let params: HashSet<&str> = rule.params.iter().map(|p| p.text.as_str()).collect();
    let mut pending: Vec<&Expr> = match &rule.body {
        RuleBody::Alternatives(alternatives) => alternatives.iter().map(|a| &a.expr).collect(),
        RuleBody::Precedence(levels) => (levels.iter())
            .flat_map(|level| level.alternatives.iter().map(|a| &a.expr))
            .collect(),
    };
    let mut found = Vec::new();
    while let Some(expr) = pending.pop() {
        if let ExprKind::Reference { name, .. } = &expr.kind
            && !params.contains(name.text.as_str())
        {
            found.push(name.text.as_str());
        }
        pending.extend(expr.children());
    }
    found
}

/// Add the rules an import takes to the grammar under their names, and the rules they need
/// under `module.name`, which no grammar can write, so that they clash with none. The levels of
/// a precedence block always take such a name, since an import takes rules, not levels.
fn import(
    grammar: &mut Grammar,
    module: Grammar,
    name: &Text,
    names: Option<Vec<Name>>,
) -> Result<(), Vec<Diagnostic>> {
    let rules: HashMap<&str, &Rule> = (module.rules.iter())
        .map(|rule| (rule.name.text.as_str(), rule))
        .collect();
    // The rule that defines each name of the module, its block for a level.
    let owners: HashMap<&str, &str> = (module.rules.iter())
        .flat_map(|rule| {
            let owner = rule.name.text.as_str();
            levels(rule)
                .map(move |level| (level, owner))
                .chain([(owner, owner)])
        })
        .collect();
    let taken: Vec<(&str, Span)> = match &names {
        Some(names) => names.iter().map(|n| (n.text.as_str(), n.span)).collect(),
        None => (module.rules.iter())
            .map(|rule| (rule.name.text.as_str(), name.span))
            .collect(),
    };
    let own: HashSet<&str> = (grammar.rules.iter())
        .flat_map(|rule| levels(rule).chain([rule.name.text.as_str()]))
        .collect();
    let mut errors = Vec::new();
    for &(rule, span) in &taken {
        if !rules.contains_key(rule) {
            let message = format!("the grammar {} defines no rule {rule}", name.value);
            errors.extend(problem(DiagnosticCode::UnknownImport, message, span));
        } else if own.contains(rule) {
            let message = format!(
                "the grammar defines {rule}, which it imports from {} too",
                name.value
            );
            errors.extend(problem(DiagnosticCode::ImportClash, message, span));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let public: HashSet<&str> = taken.iter().map(|&(rule, _)| rule).collect();
    let mut needed = public.clone();
    let mut pending: Vec<&str> = needed.iter().copied().collect();
    while let Some(rule) = pending.pop() {
        for used in references(rules[rule]) {
            if let Some(&owner) = owners.get(used)
                && needed.insert(owner)
            {
                pending.push(owner);
            }
        }
    }
    let renamed = |rule: &str| match public.contains(rule) {
        true => rule.to_owned(),
        false => format!("{}.{rule}", name.value),
    };
    for rule in &module.rules {
        if !needed.contains(rule.name.text.as_str()) {
            continue;
        }
        let params: HashSet<&str> = rule.params.iter().map(|p| p.text.as_str()).collect();
        let alternatives = |alternatives: &[Alternative]| -> Vec<Alternative> {
            (alternatives.iter())
                .map(|a| Alternative {
                    expr: rename_references(&a.expr, &params, &owners, &renamed),
                    ..a.clone()
                })
                .collect()
        };
        let body = match &rule.body {
            RuleBody::Alternatives(a) => RuleBody::Alternatives(alternatives(a)),
            RuleBody::Precedence(levels) => RuleBody::Precedence(
                (levels.iter())
                    .map(|level| Level {
                        name: Name {
                            text: renamed(&level.name.text),
                            span: level.name.span,
                        },
                        alternatives: alternatives(&level.alternatives),
                    })
                    .collect(),
            ),
        };
        let mut copy = rule.clone();
        copy.name.text = renamed(&rule.name.text);
        copy.body = body;
        grammar.rules.push(copy);
    }
    join_flags(&mut grammar.flags, module.flags);
    Ok(())
}

/// An expression with each reference to a name of the module renamed, parameters left alone.
fn rename_references(
    expr: &Expr,
    params: &HashSet<&str>,
    owners: &HashMap<&str, &str>,
    renamed: &dyn Fn(&str) -> String,
) -> Expr {
    let mut copy =
        expr.map_children(&mut |child| rename_references(child, params, owners, renamed));
    if let ExprKind::Reference { name, .. } = &mut copy.kind
        && !params.contains(name.text.as_str())
        && owners.contains_key(name.text.as_str())
    {
        name.text = renamed(&name.text);
    }
    copy
}
