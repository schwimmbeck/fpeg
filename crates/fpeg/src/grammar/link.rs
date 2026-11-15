// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Modules: a grammar joined with the grammars it extends and imports into one grammar, before
//! it resolves.
//!
//! The text of each module is read with its spans placed after those of the grammars before it,
//! so that a problem the resolver or `check` finds in a module's rules can be traced back to it.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;
use std::io::{self, Read};
use std::path::PathBuf;

use super::{
    Alternative, Expr, ExprKind, Grammar, Import, Level, Name, Rule, RuleBody, Span, Text,
};
use crate::diag::{Diagnostic, DiagnosticCode};
use crate::lines::LineIndex;

/// Modules that may extend or import each other in a chain, which bounds the recursion of
/// joining them, since modules given with a grammar can come from anywhere.
const MAX_MODULE_DEPTH: usize = 64;

/// Modules a grammar may take in together, counting a module again for each directive that
/// takes it in, so that a lattice of imports cannot multiply them without end.
const MAX_MODULES: usize = 1_000;

/// Bytes of grammar text, the grammar's own and that of the modules it takes in, counted again
/// for each directive, that joining may read together.
const MAX_MODULE_TEXT_BYTES: u32 = 64 << 20;

/// The text of a grammar that comes with the library, by the name a grammar extends or imports
/// it by.
fn bundled(name: &str) -> Option<&'static str> {
    match name {
        "common" => Some(include_str!("../../grammars/common.fpeg")),
        "json" => Some(include_str!("../../grammars/json.fpeg")),
        "python" => Some(include_str!("../../grammars/python.fpeg")),
        _ => None,
    }
}

/// Where the grammars that a grammar extends and imports come from, before those that come
/// with the library.
pub trait Modules {
    /// The text of the grammar of this name, `None` if there is none, or the error of one
    /// that cannot be read, which joining reports rather than look further.
    fn text(&self, name: &str) -> io::Result<Option<String>>;
}

/// Modules given by name with their texts.
impl<S: BuildHasher> Modules for HashMap<String, String, S> {
    fn text(&self, name: &str) -> io::Result<Option<String>> {
        Ok(self.get(name).cloned())
    }
}

/// The modules of the first, then those of the second.
impl<A: Modules, B: Modules> Modules for (A, B) {
    fn text(&self, name: &str) -> io::Result<Option<String>> {
        match self.0.text(name)? {
            Some(text) => Ok(Some(text)),
            None => self.1.text(name),
        }
    }
}

/// The grammar files of a folder: the module `m` is the file `m.fpeg`, UTF-8 with or without a
/// byte order mark. A name that is not plain names none, so that a grammar cannot reach out of
/// the folder. Only a regular file of at most the text that joining reads is read.
#[derive(Clone, Debug)]
pub struct ModuleFiles {
    folder: PathBuf,
}

impl ModuleFiles {
    /// The grammar files of a folder.
    pub fn new(folder: impl Into<PathBuf>) -> ModuleFiles {
        ModuleFiles {
            folder: folder.into(),
        }
    }
}

impl Modules for ModuleFiles {
    fn text(&self, name: &str) -> io::Result<Option<String>> {
        if !plain(name) {
            return Ok(None);
        }
        let path = self.folder.join(format!("{name}.fpeg"));
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        // A device, a pipe or a folder could block or never end.
        if !file.metadata()?.is_file() {
            return Err(io::Error::other("it is not a file"));
        }
        let mut text = String::new();
        let limit = u64::from(MAX_MODULE_TEXT_BYTES);
        file.take(limit + 1).read_to_string(&mut text)?;
        if text.len() as u64 > limit {
            let message = format!("it is larger than {} MiB", MAX_MODULE_TEXT_BYTES >> 20);
            return Err(io::Error::other(message));
        }
        Ok(Some(match text.strip_prefix('\u{feff}') {
            Some(rest) => rest.to_owned(),
            None => text,
        }))
    }
}

/// Whether a module name is plain: a letter, then letters, digits and underscores. A dot in it
/// would make the private names of two modules alike, and other characters make names that
/// no rule can have.
fn plain(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Only the grammars that come with the library.
struct Library;

impl Modules for Library {
    fn text(&self, _name: &str) -> io::Result<Option<String>> {
        Ok(None)
    }
}

/// The text of a module, where its spans start, and the directive of the grammar that took it
/// in, directly or through other modules.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Source {
    name: String,
    text: String,
    base: u32,
    directive: Span,
}

impl Grammar {
    /// The grammar joined with the grammars it extends and imports, which `modules` gives
    /// before the library. A grammar is joined by itself where it is compiled or checked, with
    /// the library's grammars alone.
    pub fn link(&self, modules: &dyn Modules) -> Result<Grammar, Vec<Diagnostic>> {
        link(self, modules).map(Cow::into_owned)
    }

    /// The diagnostics of a joined grammar, each whose span lies in a module moved to the
    /// directive that took the module in, with its message saying where in the module.
    pub fn locate(&self, diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
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
    let lines = LineIndex::new(&source.text);
    let (line, column) = lines.line_column(&source.text, (at - source.base) as usize);
    Diagnostic {
        message: format!(
            "in the grammar {}, line {line}, column {column}: {}",
            source.name, diagnostic.message
        ),
        span: source.directive,
        ..diagnostic
    }
}

/// Join a grammar with the grammars it extends and imports, from the library alone.
pub(crate) fn link_library(grammar: &Grammar) -> Result<Cow<'_, Grammar>, Vec<Diagnostic>> {
    link(grammar, &Library)
}

/// Join a grammar with the grammars it extends and imports.
fn link<'g>(
    grammar: &'g Grammar,
    modules: &dyn Modules,
) -> Result<Cow<'g, Grammar>, Vec<Diagnostic>> {
    if grammar.extends.is_none() && grammar.imports.is_empty() {
        return Ok(Cow::Borrowed(grammar));
    }
    let mut linker = Linker {
        modules,
        sources: Vec::new(),
        next_base: extent(grammar).saturating_add(1),
        reading: Vec::new(),
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

struct Linker<'m> {
    modules: &'m dyn Modules,
    sources: Vec<Source>,
    next_base: u32,
    /// The modules being joined, each with what it extends and imports, outermost first.
    reading: Vec<String>,
}

/// The error of grammars whose texts, with the names their imports write, pass the budget.
fn too_much_text(directive: Span) -> Vec<Diagnostic> {
    let message = format!(
        "the grammars extended and imported take more than {} MiB of text",
        MAX_MODULE_TEXT_BYTES >> 20
    );
    problem(DiagnosticCode::TooLarge, message, directive)
}

fn problem(code: DiagnosticCode, message: String, span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic {
        code,
        message,
        span,
    }]
}

impl Linker<'_> {
    /// A grammar joined with what it extends and imports, through any number of grammars.
    /// `directive` is the one of the grammar itself that this grammar came in through.
    fn linked(
        &mut self,
        mut grammar: Grammar,
        directive: Option<Span>,
    ) -> Result<Grammar, Vec<Diagnostic>> {
        if let Some(base) = grammar.extends.take() {
            let through = directive.unwrap_or(base.span);
            let module = self.joined(&base, through)?;
            grammar = extend(module, grammar)?;
        }
        let imports = by_module(std::mem::take(&mut grammar.imports))?;
        let mut names: HashSet<String> = (grammar.rules.iter())
            .flat_map(|rule| levels(rule).chain([rule.name.text.as_str()]))
            .map(str::to_owned)
            .collect();
        for taken in imports {
            let through = directive.unwrap_or(taken.module.span);
            let module = self.joined(&taken.module, through)?;
            let budget = MAX_MODULE_TEXT_BYTES.saturating_sub(self.next_base) as usize;
            let taken_in = (module, &taken.module, taken.names);
            let written = import(&mut grammar, &mut names, taken_in, budget)?;
            self.next_base = (u32::try_from(written).ok())
                .and_then(|written| self.next_base.checked_add(written))
                .filter(|&end| end <= MAX_MODULE_TEXT_BYTES)
                .ok_or_else(|| too_much_text(through))?;
        }
        Ok(grammar)
    }

    /// A module joined with what it extends and imports in turn.
    fn joined(&mut self, name: &Text, directive: Span) -> Result<Grammar, Vec<Diagnostic>> {
        if self.reading.contains(&name.value) {
            let message = format!(
                "the grammar {} takes itself in through the grammars it extends and imports",
                name.value
            );
            return Err(problem(DiagnosticCode::ModuleCycle, message, directive));
        }
        if self.reading.len() == MAX_MODULE_DEPTH {
            let message = format!(
                "the grammars extend and import each other more than {MAX_MODULE_DEPTH} deep"
            );
            return Err(problem(DiagnosticCode::TooDeep, message, directive));
        }
        if self.sources.len() == MAX_MODULES {
            let message = format!(
                "the grammar takes in more than {MAX_MODULES} grammars through those it extends \
                 and imports"
            );
            return Err(problem(DiagnosticCode::TooLarge, message, directive));
        }
        let module = self.read(name, directive)?;
        self.reading.push(name.value.clone());
        let joined = self.linked(module, Some(directive));
        self.reading.pop();
        joined
    }

    /// The grammar model of a module.
    fn read(&mut self, name: &Text, directive: Span) -> Result<Grammar, Vec<Diagnostic>> {
        if !plain(&name.value) {
            let message = format!(
                "a module name is a letter, then letters, digits and underscores, not {}",
                name.value
            );
            return Err(problem(DiagnosticCode::UnknownModule, message, name.span));
        }
        let text = self.modules.text(&name.value).map_err(|error| {
            let message = format!("the grammar {} cannot be read: {error}", name.value);
            problem(DiagnosticCode::UnknownModule, message, name.span)
        })?;
        let Some(text) = text.or_else(|| bundled(&name.value).map(str::to_owned)) else {
            let message = format!("no grammar is named {}", name.value);
            return Err(problem(DiagnosticCode::UnknownModule, message, name.span));
        };
        let base = self.next_base;
        let too_large = || too_much_text(directive);
        let length = u32::try_from(text.len()).map_err(|_| too_large())?;
        self.next_base = (base.checked_add(length))
            .and_then(|end| end.checked_add(1))
            .filter(|&end| end <= MAX_MODULE_TEXT_BYTES)
            .ok_or_else(too_large)?;
        let module = Grammar::read_module(&text, base);
        self.sources.push(Source {
            name: name.value.clone(),
            text,
            base,
            directive,
        });
        module
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
    let mut words: HashSet<String> = (base.soft_keywords.iter())
        .map(|w| w.value.clone())
        .collect();
    for word in extension.soft_keywords {
        if words.insert(word.value.clone()) {
            base.soft_keywords.push(word);
        }
    }
    base.imports.extend(extension.imports);
    base.recovers.extend(extension.recovers);
    Ok(base)
}

/// The flags of a grammar with those of another added, each name once.
fn join_flags(flags: &mut Vec<Name>, more: Vec<Name>) {
    let mut names: HashSet<String> = flags.iter().map(|f| f.text.clone()).collect();
    for flag in more {
        if names.insert(flag.text.clone()) {
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
    if alternatives[at].builder.is_some() {
        let message = "... keeps the builders of the inherited alternatives and takes none";
        return Err(misplaced(span, message.to_owned()));
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
    let mut index: HashMap<String, usize> = HashMap::new();
    for import in imports {
        if let Some(prefix) = &import.prefix {
            let message = "an import under a prefix is not supported yet, see the roadmap (M9)";
            return Err(problem(DiagnosticCode::NotYet, message.into(), prefix.span));
        }
        let names = (!import.names.is_empty()).then_some(import.names);
        match index.get(&import.module.value) {
            Some(&i) => match (modules[i].names.as_mut(), names) {
                (Some(taken), Some(more)) => taken.extend(more),
                _ => modules[i].names = None,
            },
            None => {
                index.insert(import.module.value.clone(), modules.len());
                modules.push(Taken {
                    module: import.module,
                    names,
                });
            }
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

/// What the alternatives of a rule use.
struct Uses<'r> {
    /// The names it refers to, rules and levels, its parameters left out.
    names: Vec<&'r Name>,
    /// The flags it sets or tests.
    flags: Vec<&'r str>,
}

fn uses(rule: &Rule) -> Uses<'_> {
    let params: HashSet<&str> = rule.params.iter().map(|p| p.text.as_str()).collect();
    let mut pending: Vec<&Expr> = match &rule.body {
        RuleBody::Alternatives(alternatives) => alternatives.iter().map(|a| &a.expr).collect(),
        RuleBody::Precedence(levels) => (levels.iter())
            .flat_map(|level| level.alternatives.iter().map(|a| &a.expr))
            .collect(),
    };
    let mut used = Uses {
        names: Vec::new(),
        flags: Vec::new(),
    };
    while let Some(expr) = pending.pop() {
        match &expr.kind {
            ExprKind::Reference { name, .. } if !params.contains(name.text.as_str()) => {
                used.names.push(name);
            }
            ExprKind::FlagOn { flag, .. }
            | ExprKind::FlagOff { flag, .. }
            | ExprKind::FlagTest { flag, .. } => used.flags.push(flag.text.as_str()),
            _ => {}
        }
        pending.extend(expr.children());
    }
    used
}

/// Add the rules an import takes to the grammar under their names, and the rules they need
/// under `module.name`, which no grammar can write, so that they clash with none. The levels of
/// a precedence block always take such a name, since an import takes rules, not levels. Such
/// a name can only come from that rule of that module, so a copy that the grammar has already,
/// through what it extends or another import, is shared. `names` holds the names of the
/// grammar's rules and levels. The bytes of the private names it writes come back, since a
/// long module name times many references can take far more than the module's text, and it
/// writes no more names once they pass `budget`.
fn import(
    grammar: &mut Grammar,
    names: &mut HashSet<String>,
    (module, name, wanted): (Grammar, &Text, Option<Vec<Name>>),
    budget: usize,
) -> Result<usize, Vec<Diagnostic>> {
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
    // Without names an import takes the module's own rules, not those private to it.
    let taken: Vec<(&str, Span)> = match &wanted {
        Some(wanted) => wanted.iter().map(|n| (n.text.as_str(), n.span)).collect(),
        None => (module.rules.iter())
            .filter(|rule| !rule.name.text.contains('.'))
            .map(|rule| (rule.name.text.as_str(), name.span))
            .collect(),
    };
    let mut errors = Vec::new();
    for &(rule, span) in &taken {
        if !rules.contains_key(rule) {
            let message = format!("the grammar {} defines no rule {rule}", name.value);
            errors.extend(problem(DiagnosticCode::UnknownImport, message, span));
        } else if names.contains(rule) {
            let message = format!(
                "the grammar has a rule {rule} already, which it imports from {} too",
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
    let mut flags: HashSet<&str> = HashSet::new();
    while let Some(rule) = pending.pop() {
        let used = uses(rules[rule]);
        flags.extend(used.flags);
        for name in used.names {
            match owners.get(name.text.as_str()) {
                Some(&owner) => {
                    if needed.insert(owner) {
                        pending.push(owner);
                    }
                }
                // A name its module lacks would take a rule of the grammar that imports it.
                None => {
                    let message = format!("unknown rule {}", name.text);
                    errors.extend(problem(DiagnosticCode::UnknownRule, message, name.span));
                }
            }
        }
    }
    if !errors.is_empty() {
        errors.sort_by_key(|error| error.span.start);
        return Err(errors);
    }
    let written = std::cell::Cell::new(0usize);
    let renamed = |rule: &str| {
        if public.contains(rule) {
            return rule.to_owned();
        }
        let total = written
            .get()
            .saturating_add(name.value.len() + 1 + rule.len());
        written.set(total);
        match total <= budget {
            true => format!("{}.{rule}", name.value),
            false => String::new(),
        }
    };
    for rule in &module.rules {
        if !needed.contains(rule.name.text.as_str()) {
            continue;
        }
        let new_name = renamed(&rule.name.text);
        if !names.insert(new_name.clone()) {
            continue;
        }
        let params: HashSet<&str> = rule.params.iter().map(|p| p.text.as_str()).collect();
        // The kinds that a private rule's aliases name are private too.
        let private = !public.contains(rule.name.text.as_str());
        let alias = |alias: &Name| match private {
            true => Name {
                text: format!("{}.{}", name.value, alias.text),
                span: alias.span,
            },
            false => alias.clone(),
        };
        let alternatives = |alternatives: &[Alternative]| -> Vec<Alternative> {
            (alternatives.iter())
                .map(|a| Alternative {
                    expr: rename_references(&a.expr, &params, &owners, &renamed),
                    alias: a.alias.as_ref().map(alias),
                    builder: a.builder.clone(),
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
        names.extend(levels(rule).map(&renamed));
        let mut copy = rule.clone();
        copy.name.text = new_name;
        copy.body = body;
        grammar.rules.push(copy);
    }
    let used: Vec<Name> = (module.flags.into_iter())
        .filter(|flag| flags.contains(flag.text.as_str()))
        .collect();
    join_flags(&mut grammar.flags, used);
    Ok(written.get())
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
