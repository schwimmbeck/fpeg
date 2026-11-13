// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Resolve a grammar: check its names and translate it into the core language, following the
//! translation steps of the semantics page.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use crate::diag::DiagnosticCode;
use crate::grammar::print::{builder_text, expr_text};
use crate::grammar::{
    Alternative, BUILT_IN_BUILDERS, BuilderValue, ClassItem, ErrorLabel, Expr, ExprKind, Grammar,
    GrammarError, Level, MAX_EXPRESSION_DEPTH, Name, Rule, RuleBody, RuleKind, Span, ValueBuilder,
};
use crate::ir::{self, CaptureId, CharSet, FlagId, LabelId, RuleId};
use crate::items::TooLarge;
use crate::tree::{FieldId, KindId, Names};

/// Template instances resolving may create before it gives up. Growing arguments meet the depth
/// limit or the expression budget first, this bounds many small instances.
const MAX_TEMPLATE_INSTANCES: usize = 1_000;

/// Expressions all template instances may expand into together, counting a copy of an argument
/// for every use of its parameter, so that a small grammar cannot take the memory of a huge one.
const MAX_INSTANCE_EXPRESSIONS: usize = 100_000;

/// Flags a grammar may declare. The engine keeps the flags in one 64-bit register.
pub(crate) const MAX_FLAGS: usize = 64;

/// The tab width when a grammar sets none.
const DEFAULT_TAB_WIDTH: u32 = 8;

/// The widest tab a grammar may set, far beyond any real layout, which keeps columns countable.
const MAX_TAB_WIDTH: u32 = 1_000;

/// Errors resolving reports at most. More would not help a reader fix the first ones.
pub(crate) const MAX_ERRORS: usize = 100;

/// The attributes a rule may carry.
const ATTRIBUTES: [&str; 5] = ["lexical", "reserved", "sync", "memo", "nomemo"];

/// The built-in builders that make a value of a node's children, which a token does not have.
const CHILDREN_BUILDERS: [&str; 3] = ["list", "tuple", "dict"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Syntactic,
    Lexical,
    Plain,
}

/// Where an expression is lowered: the mode, whether it stands directly inside a field, the
/// arguments of the template instance with how many expressions each holds, and the level
/// names of a precedence block.
#[derive(Clone, Copy)]
struct Scope<'s> {
    mode: Mode,
    in_field: bool,
    params: &'s HashMap<String, Expr>,
    sizes: &'s HashMap<String, usize>,
    levels: Option<&'s HashMap<String, RuleId>>,
}

struct Pending<'g> {
    id: RuleId,
    rule: &'g Rule,
    args: Vec<Expr>,
}

/// A field and the parts of its expression that one match captures from, checked once every
/// rule is lowered, since the parts may call rules not lowered yet.
struct FieldCheck {
    name: String,
    span: Span,
    parts: Vec<ir::Expr>,
}

/// The fields a builder names and the alternative it belongs to, checked once every rule is
/// lowered, since the alternative's fields may come from rules it calls.
struct BuilderCheck {
    rule: String,
    fields: Vec<Name>,
    alternative: ir::Expr,
}

#[derive(Default)]
struct Interner {
    names: Vec<String>,
    ids: HashMap<String, u32>,
}

impl Interner {
    fn with(first: &[&str]) -> Interner {
        let mut interner = Interner::default();
        for name in first {
            interner.id(name);
        }
        interner
    }

    fn id(&mut self, name: &str) -> (u32, bool) {
        if let Some(&id) = self.ids.get(name) {
            return (id, false);
        }
        let id = self.names.len() as u32;
        self.names.push(name.to_owned());
        self.ids.insert(name.to_owned(), id);
        (id, true)
    }
}

struct Resolver<'g> {
    grammar: &'g Grammar,
    by_name: HashMap<&'g str, &'g Rule>,
    /// The levels of the precedence blocks, which every rule may call, with the level that owns
    /// each name.
    levels: HashMap<&'g str, (RuleId, &'g Level)>,
    rules: Vec<Option<ir::Rule>>,
    instances: HashMap<(String, String), RuleId>,
    /// Instances of rules without parameters, one per name, made before any template instance.
    plain_instances: usize,
    queue: VecDeque<Pending<'g>>,
    /// Kinds by name and builder, so that each builder has a kind of its own.
    kinds: Interner,
    kind_names: Vec<String>,
    kind_display: Vec<String>,
    kind_builders: Vec<Option<ValueBuilder>>,
    kind_tokens: Vec<bool>,
    fields: Interner,
    flags: HashMap<&'g str, FlagId>,
    captures: Interner,
    labels: Vec<ir::Label>,
    label_ids: HashMap<ir::Label, LabelId>,
    word: CharSet,
    soft: HashSet<&'g str>,
    reserved: ir::Reserved,
    field_checks: Vec<FieldCheck>,
    builder_checks: Vec<BuilderCheck>,
    /// Expressions in the arguments and bodies of the template instances made so far.
    instance_expressions: usize,
    errors: Vec<GrammarError>,
}

/// Resolve a grammar into the core language, or report the problems found, the first hundred at
/// most.
pub(crate) fn resolve(grammar: &Grammar) -> Result<ir::Grammar, Vec<GrammarError>> {
    let mut r = Resolver {
        grammar,
        by_name: HashMap::new(),
        levels: HashMap::new(),
        rules: Vec::new(),
        instances: HashMap::new(),
        plain_instances: 0,
        queue: VecDeque::new(),
        kinds: Interner::with(&["<error>", "<missing>", "<skipped>"]),
        kind_names: vec!["<error>".into(), "<missing>".into(), "<skipped>".into()],
        kind_display: vec!["error".into(), "missing".into(), "skipped".into()],
        kind_builders: vec![None, None, None],
        kind_tokens: vec![false, true, true],
        fields: Interner::with(&[""]),
        flags: HashMap::new(),
        captures: Interner::default(),
        labels: Vec::new(),
        label_ids: HashMap::new(),
        word: CharSet {
            negated: false,
            items: vec![
                ClassItem::Range('A', 'Z'),
                ClassItem::Range('a', 'z'),
                ClassItem::Range('0', '9'),
                ClassItem::Range('_', '_'),
            ],
        },
        soft: grammar
            .soft_keywords
            .iter()
            .map(|t| t.value.as_str())
            .collect(),
        reserved: ir::Reserved::default(),
        field_checks: Vec::new(),
        builder_checks: Vec::new(),
        instance_expressions: 0,
        errors: Vec::new(),
    };
    r.directives();
    for rule in &grammar.rules {
        r.check_name(&rule.name);
        if r.by_name.insert(&rule.name.text, rule).is_some() {
            r.error(
                DiagnosticCode::Duplicate,
                format!("the rule {} is defined twice", rule.name.text),
                rule.name.span,
            );
        }
    }
    // Level names belong to the whole grammar, so their rules exist before any rule calls one.
    // A block with parameters is an error, reported here so that an unused one is reported too,
    // and keeps its levels to itself.
    for rule in &grammar.rules {
        let RuleBody::Precedence(levels) = &rule.body else {
            continue;
        };
        if !rule.params.is_empty() {
            let message = format!(
                "the precedence block {} cannot have parameters",
                rule.name.text
            );
            r.error(DiagnosticCode::Precedence, message, rule.name.span);
            continue;
        }
        for level in levels {
            let name = level.name.text.as_str();
            if r.by_name.contains_key(name) {
                r.error(
                    DiagnosticCode::Duplicate,
                    format!("the level {name} has the name of a rule"),
                    level.name.span,
                );
            } else if r.levels.contains_key(name) {
                r.error(
                    DiagnosticCode::Duplicate,
                    format!("the level {name} is defined twice"),
                    level.name.span,
                );
            } else {
                let id = r.allocate();
                r.levels.insert(name, (id, level));
            }
        }
    }
    let mut entry = HashMap::new();
    for rule in &grammar.rules {
        if rule.params.is_empty() {
            r.instance(rule, Vec::new(), rule.name.span);
        }
    }
    r.plain_instances = r.instances.len();
    while let Some(pending) = r.queue.pop_front() {
        r.lower_rule(pending);
    }
    let (no_params, no_sizes) = (HashMap::new(), HashMap::new());
    let plain = Scope {
        mode: Mode::Plain,
        in_field: false,
        params: &no_params,
        sizes: &no_sizes,
        levels: None,
    };
    let ignored: Vec<&Expr> = grammar.ignore.iter().collect();
    let skip = match grammar.ignore.as_slice() {
        [] => None,
        _ if r.too_deep(&ignored, "%ignore", Span::default()) => None,
        items => Some(choice(items.iter().map(|e| r.lower(e, plain)).collect())),
    };
    let syntactic = Scope {
        mode: Mode::Syntactic,
        ..plain
    };
    for rule in &grammar.rules {
        if rule.params.is_empty() {
            let start = r.reference(&rule.name, &[], syntactic, rule.name.span);
            entry.insert(rule.name.text.clone(), start);
        }
    }
    // A reference reaches a level as it reaches a rule, so a parse may start from one.
    let levels: Vec<&Name> = r.levels.values().map(|&(_, level)| &level.name).collect();
    for name in levels {
        let start = r.reference(name, &[], syntactic, name.span);
        entry.insert(name.text.clone(), start);
    }
    while let Some(pending) = r.queue.pop_front() {
        r.lower_rule(pending);
    }
    r.check_fields();
    let counts = match crate::items::Counts::new(&r.rules) {
        Ok(counts) => counts,
        Err(too_large) => {
            let (TooLarge::Fields(rule) | TooLarge::Steps(rule)) = too_large;
            let (name, span) = (r.rules[rule].as_ref()).map_or(("", Span::default()), |rule| {
                (rule.name.as_str(), rule.span)
            });
            let message = match too_large {
                TooLarge::Fields(_) => format!(
                    "the rule {name} passes on more than {} fields to a node",
                    crate::items::MAX_NODE_FIELDS
                ),
                TooLarge::Steps(_) => format!(
                    "the fields that the rule {name} passes on take more than {} steps to count",
                    crate::items::MAX_COUNT_STEPS
                ),
            };
            r.error(DiagnosticCode::TooLarge, message, span);
            return Err(r.errors);
        }
    };
    r.check_builders(&counts);
    if !r.errors.is_empty() {
        return Err(r.errors);
    }
    let facts = counts.facts(&r.rules, &entry, r.kind_names.len());
    let rules = r.rules.into_iter().flatten().collect();
    Ok(ir::Grammar {
        rules,
        entry,
        kind_fields: facts.kind_fields,
        kind_builders: r.kind_builders,
        kind_tokens: r.kind_tokens,
        kind_made: facts.kind_made,
        single_item: facts.single_item,
        names: Arc::new(Names {
            kinds: r.kind_names,
            fields: r.fields.names,
        }),
        kind_display: r.kind_display,
        skip,
        word: r.word,
        reserved: r.reserved,
        tab_width: grammar.tabs.map_or(DEFAULT_TAB_WIDTH, |t| t.value),
        labels: r.labels,
    })
}

fn choice(mut items: Vec<ir::Expr>) -> ir::Expr {
    if items.len() == 1 {
        items.remove(0)
    } else {
        ir::Expr::Choice(items)
    }
}

/// Counts how many visible items without a field one match of an expression adds, at most,
/// where 2 stands for several.
struct ItemCount<'r> {
    /// The same count for each rule.
    rules: &'r [u8],
    /// The rules called where their count adds to the expression's.
    calls: Vec<usize>,
    /// The count of each shared part met so far. Nested separated repetitions use their item
    /// twice at every level, so without it the time doubles with each level.
    shared: HashMap<*const ir::Expr, u8>,
}

impl ItemCount<'_> {
    fn new(rules: &[u8]) -> ItemCount<'_> {
        ItemCount {
            rules,
            calls: Vec::new(),
            shared: HashMap::new(),
        }
    }

    fn of(&mut self, expr: &ir::Expr) -> u8 {
        use ir::Expr as E;
        match expr {
            E::Token { hidden, .. } => u8::from(!hidden),
            E::Node { .. } | E::Transparent { .. } => 1,
            E::Call(id) => {
                self.calls.push(id.0 as usize);
                self.rules[id.0 as usize]
            }
            E::Seq(items) => items.iter().fold(0, |sum, e| (sum + self.of(e)).min(2)),
            E::Choice(items) => items.iter().map(|e| self.of(e)).max().unwrap_or(0),
            E::Repeat { max: Some(0), .. } => 0,
            E::Repeat { expr, max, .. } => match (self.of(expr), max) {
                (0, _) => 0,
                (n, Some(1)) => n,
                _ => 2,
            },
            E::Unreserved(expr)
            | E::FlagOn { expr, .. }
            | E::FlagOff { expr, .. }
            | E::Indent { expr, .. }
            | E::Def { expr, .. }
            | E::Label { expr, .. } => self.of(expr),
            E::Shared(expr) => {
                let key = Arc::as_ptr(expr);
                if let Some(&count) = self.shared.get(&key) {
                    return count;
                }
                let count = self.of(expr);
                self.shared.insert(key, count);
                count
            }
            // An inner field keeps its items, and lookahead adds none.
            _ => 0,
        }
    }
}

/// For each rule, how many visible items without a field one match of it adds, at most, where
/// 2 stands for several. A rule reaches a count through its own body or through a rule it calls
/// where that rule's count adds to its own, so two rounds of reachability, for one item and then
/// for several, take linear time. Repeating passes until nothing changes would take a pass per
/// rule of a chain.
fn unfielded_counts(rules: &[Option<ir::Rule>]) -> Vec<u8> {
    let mut counts = vec![0; rules.len()];
    let mut callers = vec![Vec::new(); rules.len()];
    for level in 1..=2 {
        let mut reached = Vec::new();
        for (id, rule) in rules.iter().enumerate() {
            let Some(rule) = rule else { continue };
            let mut count = ItemCount::new(&counts);
            if count.of(&rule.body) >= level {
                reached.push(id);
            }
            if level == 1 {
                for callee in count.calls {
                    callers[callee].push(id);
                }
            }
        }
        while let Some(id) = reached.pop() {
            if counts[id] < level {
                counts[id] = level;
                reached.extend_from_slice(&callers[id]);
            }
        }
    }
    counts
}

/// How many expressions `expr` holds once each parameter is replaced by an argument holding
/// `sizes[parameter]` expressions, worked out without building it and saturating.
fn substituted_size(expr: &Expr, sizes: &HashMap<String, usize>) -> usize {
    let mut pending = vec![expr];
    let mut size = 0usize;
    while let Some(expr) = pending.pop() {
        // A parameter given arguments is an error, but it is counted as a use all the same.
        if let ExprKind::Reference { name, .. } = &expr.kind
            && let Some(&argument) = sizes.get(&name.text)
        {
            size = size.saturating_add(argument);
            continue;
        }
        size = size.saturating_add(1);
        pending.extend(expr.children());
    }
    size
}

/// A parameter that `expr` uses with arguments, which parameters do not take.
fn misused_parameter<'e>(expr: &'e Expr, params: &HashMap<String, Expr>) -> Option<&'e Name> {
    let mut pending = vec![expr];
    while let Some(expr) = pending.pop() {
        if let ExprKind::Reference { name, args } = &expr.kind
            && !args.is_empty()
            && params.contains_key(&name.text)
        {
            return Some(name);
        }
        pending.extend(expr.children());
    }
    None
}

/// Replace references to parameters by their arguments.
fn substitute(expr: &Expr, params: &HashMap<String, Expr>) -> Expr {
    if params.is_empty() {
        return expr.clone();
    }
    if let ExprKind::Reference { name, args } = &expr.kind
        && args.is_empty()
        && let Some(arg) = params.get(&name.text)
    {
        return arg.clone();
    }
    expr.map_children(&mut |child| substitute(child, params))
}

impl<'g> Resolver<'g> {
    fn error(&mut self, code: DiagnosticCode, message: String, span: Span) {
        // Each instance of a template reports the errors of its body again. The cap keeps the
        // check for those repeats cheap.
        let error = GrammarError {
            code,
            message,
            span,
        };
        if self.errors.len() < MAX_ERRORS && !self.errors.contains(&error) {
            self.errors.push(error);
        }
    }

    /// Report fields that can capture several items in one match.
    fn check_fields(&mut self) {
        let counts = unfielded_counts(&self.rules);
        for check in std::mem::take(&mut self.field_checks) {
            if (check.parts.iter()).any(|p| ItemCount::new(&counts).of(p) > 1) {
                let message = format!(
                    "the field {} can capture several items in one match",
                    check.name
                );
                self.error(DiagnosticCode::FieldItems, message, check.span);
            }
        }
    }

    fn check_name(&mut self, name: &Name) {
        if !name
            .text
            .trim_start_matches('_')
            .starts_with(|c: char| c.is_ascii_alphabetic())
        {
            let message = format!(
                "the name {} must start with a letter after any underscores",
                name.text
            );
            self.error(DiagnosticCode::Name, message, name.span);
        }
    }

    fn directives(&mut self) {
        let g = self.grammar;
        for flag in &g.flags {
            if self.flags.contains_key(flag.text.as_str()) {
                continue;
            }
            if self.flags.len() == MAX_FLAGS {
                let message = format!("a grammar declares at most {MAX_FLAGS} flags");
                self.error(DiagnosticCode::TooLarge, message, flag.span);
                break;
            }
            let next = FlagId(self.flags.len() as u16);
            self.flags.insert(&flag.text, next);
        }
        if let Some(word) = &g.word {
            match &word.kind {
                ExprKind::Class(class) => {
                    self.check_class(&class.items, word.span);
                    self.word = CharSet {
                        negated: class.negated,
                        items: class.items.clone(),
                    };
                }
                _ => self.error(
                    DiagnosticCode::Class,
                    "%word needs a character class".into(),
                    word.span,
                ),
            }
        }
        if let Some(tabs) = g.tabs
            && !(1..=MAX_TAB_WIDTH).contains(&tabs.value)
        {
            let message = format!("%tabs needs a width from 1 to {MAX_TAB_WIDTH}");
            self.error(DiagnosticCode::Bounds, message, tabs.span);
        }
        if let Some(module) = &g.extends {
            self.error(
                DiagnosticCode::NotYet,
                "%extends is not supported yet, see the roadmap (M7)".into(),
                module.span,
            );
        }
        for import in &g.imports {
            let message = "%import is not supported yet, see the roadmap (M7)".into();
            self.error(DiagnosticCode::NotYet, message, import.module.span);
        }
        for recover in &g.recovers {
            let message = "%recover is not supported yet, see the roadmap (M8)".into();
            self.error(DiagnosticCode::NotYet, message, recover.label.span);
        }
    }

    fn too_large(&mut self, template: &str, span: Span) {
        let message = format!("the instances of {template} expand into too many expressions");
        self.error(DiagnosticCode::TooLarge, message, span);
    }

    /// The id of the instance of `rule` with these closed arguments, created on first use, or
    /// none, with an error, when the grammar may not have another instance.
    fn instance(&mut self, rule: &'g Rule, args: Vec<Expr>, span: Span) -> Option<RuleId> {
        let key: Vec<String> = args.iter().map(expr_text).collect();
        let key = (rule.name.text.clone(), key.join("\u{1}"));
        if let Some(&id) = self.instances.get(&key) {
            return Some(id);
        }
        if !args.is_empty() {
            let templates = self.instances.len() - self.plain_instances;
            if templates >= MAX_TEMPLATE_INSTANCES {
                let message = format!("the template {} creates too many instances", rule.name.text);
                self.error(DiagnosticCode::TooLarge, message, span);
                return None;
            }
            // The instance lowers a copy of an argument for every use of its parameter.
            let sizes: HashMap<String, usize> = (rule.params.iter())
                .map(|p| p.text.clone())
                .zip(args.iter().map(|a| a.measure().0))
                .collect();
            // A precedence block with parameters is an error, but each instance lowers it all.
            let alternatives: Vec<&Alternative> = match &rule.body {
                RuleBody::Alternatives(alternatives) => alternatives.iter().collect(),
                RuleBody::Precedence(levels) => {
                    levels.iter().flat_map(|l| &l.alternatives).collect()
                }
            };
            let expanded = (alternatives.iter())
                .map(|a| substituted_size(&a.expr, &sizes))
                .fold(0, usize::saturating_add);
            self.instance_expressions = self.instance_expressions.saturating_add(expanded);
            if self.instance_expressions > MAX_INSTANCE_EXPRESSIONS {
                self.too_large(&rule.name.text, span);
                return None;
            }
        }
        let id = self.allocate();
        self.instances.insert(key, id);
        self.queue.push_back(Pending { id, rule, args });
        Some(id)
    }

    fn allocate(&mut self) -> RuleId {
        self.rules.push(None);
        RuleId((self.rules.len() - 1) as u32)
    }

    /// The kind of the tokens or nodes named `name` that the builder makes values of. Each
    /// builder gets a kind of its own, so that a tree finds the builder by the kind, and the
    /// tokens of a terminal and the nodes of an alias of the same name get kinds apart.
    fn kind(
        &mut self,
        name: &str,
        display: &str,
        builder: Option<&ValueBuilder>,
        token: bool,
    ) -> KindId {
        // Names hold no NUL, so no key can be another's.
        let made = if token { "token" } else { "node" };
        let key = match builder {
            None => format!("{name}\0{made}"),
            Some(builder) => format!("{name}\0{made}\0{}", builder_text(builder)),
        };
        let (id, new) = self.kinds.id(&key);
        if new {
            self.kind_names.push(name.to_owned());
            self.kind_display.push(display.to_owned());
            self.kind_builders.push(builder.cloned());
            self.kind_tokens.push(token);
        }
        KindId(id)
    }

    /// Report a builder that cannot apply where it stands or whose arguments do not fit, and
    /// keep the fields it names to check against its alternative. `owner` is the rule or level
    /// of the alternative, and `alternative` the alternative lowered, without its node.
    fn check_builder(
        &mut self,
        rule: &Rule,
        owner: &str,
        builder: &ValueBuilder,
        makes_node: bool,
        alternative: &ir::Expr,
    ) {
        let rule_name = owner;
        let terminal = rule.kind() == RuleKind::Terminal;
        let (name, args) = match builder {
            ValueBuilder::Constant(_) => (None, &[][..]),
            ValueBuilder::Name(name) => (Some(name), &[][..]),
            ValueBuilder::Call { name, args } => (Some(name), &args[..]),
        };
        if !makes_node && !terminal {
            let span = name.map_or(rule.span, |n| n.span);
            let message = format!(
                "the builder of {rule_name} has no node to make a value of, since the \
                 alternative makes none"
            );
            self.error(DiagnosticCode::BuilderPlace, message, span);
            return;
        }
        if let Some(name) = name {
            if terminal && args.is_empty() && CHILDREN_BUILDERS.contains(&name.text.as_str()) {
                let message = format!(
                    "the builder {} needs children, which the terminal {rule_name} does not have",
                    name.text
                );
                self.error(DiagnosticCode::BuilderPlace, message, name.span);
            }
            if !args.is_empty() && BUILT_IN_BUILDERS.contains(&name.text.as_str()) {
                let message = format!("the builder {} takes no arguments", name.text);
                self.error(DiagnosticCode::BuilderArguments, message, name.span);
            }
        }
        let mut keys: Vec<&str> = Vec::new();
        let mut fields = Vec::new();
        for arg in args {
            match &arg.key {
                Some(key) if keys.contains(&key.text.as_str()) => {
                    let message = format!(
                        "the keyword {} is given twice in the builder of {rule_name}",
                        key.text
                    );
                    self.error(DiagnosticCode::BuilderArguments, message, key.span);
                }
                Some(key) => keys.push(&key.text),
                None if !keys.is_empty() => {
                    let message = format!(
                        "a positional argument follows a keyword in the builder of {rule_name}"
                    );
                    let span = name.map_or(rule.span, |n| n.span);
                    self.error(DiagnosticCode::BuilderArguments, message, span);
                }
                None => {}
            }
            match &arg.value {
                BuilderValue::Field(field) if terminal => {
                    let message = format!(
                        "the builder of the terminal {rule_name} names the field {}, and a \
                         terminal has no fields",
                        field.text
                    );
                    self.error(DiagnosticCode::BuilderPlace, message, field.span);
                }
                BuilderValue::Field(field) => fields.push(field.clone()),
                BuilderValue::All if terminal => {
                    let message = format!(
                        "the builder of the terminal {rule_name} takes all children with *, \
                         which a terminal does not have"
                    );
                    let span = name.map_or(rule.span, |n| n.span);
                    self.error(DiagnosticCode::BuilderPlace, message, span);
                }
                BuilderValue::All | BuilderValue::Constant(_) => {}
            }
        }
        if !fields.is_empty() {
            self.builder_checks.push(BuilderCheck {
                rule: rule_name.to_owned(),
                fields,
                alternative: alternative.clone(),
            });
        }
    }

    /// Report fields that builders name and their alternatives cannot hold.
    fn check_builders(&mut self, counts: &crate::items::Counts) {
        for check in std::mem::take(&mut self.builder_checks) {
            let held = counts.fields(&check.alternative);
            for field in &check.fields {
                let known =
                    (self.fields.ids.get(&field.text)).and_then(|&id| u16::try_from(id).ok());
                if !known.is_some_and(|id| held.contains(&FieldId(id))) {
                    let message = format!(
                        "the builder of {} names the field {}, which its alternative cannot hold",
                        check.rule, field.text
                    );
                    self.error(DiagnosticCode::BuilderField, message, field.span);
                }
            }
        }
    }

    fn field(&mut self, name: &Name) -> FieldId {
        let (id, _) = self.fields.id(&name.text);
        match u16::try_from(id) {
            Ok(id) => FieldId(id),
            Err(_) => {
                self.error(
                    DiagnosticCode::TooLarge,
                    "the grammar has too many field names".into(),
                    name.span,
                );
                FieldId::NONE
            }
        }
    }

    fn capture(&mut self, name: &Name) -> CaptureId {
        let (id, _) = self.captures.id(&name.text);
        match u16::try_from(id) {
            Ok(id) => CaptureId(id),
            Err(_) => {
                self.error(
                    DiagnosticCode::TooLarge,
                    "the grammar has too many capture names".into(),
                    name.span,
                );
                CaptureId(0)
            }
        }
    }

    fn flag(&mut self, name: &Name) -> FlagId {
        match self.flags.get(name.text.as_str()) {
            Some(&id) => id,
            None => {
                // A declared flag beyond the limit has its error at the limit already.
                if !self.grammar.flags.iter().any(|f| f.text == name.text) {
                    let message = format!("the flag {} is not declared with %flag", name.text);
                    self.error(DiagnosticCode::UnknownFlag, message, name.span);
                }
                FlagId(0)
            }
        }
    }

    fn label(&mut self, label: ir::Label) -> LabelId {
        if let Some(&id) = self.label_ids.get(&label) {
            return id;
        }
        let id = LabelId(self.labels.len() as u32);
        self.labels.push(label.clone());
        self.label_ids.insert(label, id);
        id
    }

    fn check_class(&mut self, items: &[ClassItem], span: Span) {
        if items.is_empty() {
            self.error(
                DiagnosticCode::Class,
                "a character class must list something".into(),
                span,
            );
        }
        for item in items {
            if let ClassItem::Range(first, last) = item
                && first > last
            {
                let message = format!("the range {first:?}-{last:?} runs backwards");
                self.error(DiagnosticCode::Class, message, span);
            }
        }
    }

    /// Report an expression that nests too deeply for the passes that recurse over it.
    fn too_deep(&mut self, exprs: &[&Expr], what: &str, span: Span) -> bool {
        let deep = exprs.iter().any(|e| e.measure().1 > MAX_EXPRESSION_DEPTH);
        if deep {
            self.error(
                DiagnosticCode::TooDeep,
                format!("{what} nests deeper than {MAX_EXPRESSION_DEPTH} levels"),
                span,
            );
        }
        deep
    }

    fn lower_rule(&mut self, pending: Pending<'g>) {
        let Pending { id, rule, args } = pending;
        let exprs: Vec<&Expr> = match &rule.body {
            RuleBody::Alternatives(alternatives) => alternatives.iter().map(|a| &a.expr).collect(),
            RuleBody::Precedence(levels) => levels
                .iter()
                .flat_map(|l| l.alternatives.iter().map(|a| &a.expr))
                .collect(),
        };
        if self.too_deep(
            &exprs,
            &format!("the rule {}", rule.name.text),
            rule.name.span,
        ) {
            self.rules[id.0 as usize] = Some(ir::Rule {
                name: rule.name.text.clone(),
                span: rule.name.span,
                body: ir::Expr::Empty,
                memo: ir::MemoMark::None,
                level: false,
            });
            return;
        }
        let kind = rule.kind();
        for attribute in &rule.attributes {
            if !ATTRIBUTES.contains(&attribute.text.as_str()) {
                self.error(
                    DiagnosticCode::UnknownAttribute,
                    format!("unknown attribute @{}", attribute.text),
                    attribute.span,
                );
            }
        }
        if kind == RuleKind::Terminal && rule.has_attribute("lexical") {
            let message = format!("the terminal {} cannot be @lexical", rule.name.text);
            self.error(DiagnosticCode::RuleForm, message, rule.name.span);
        }
        if kind != RuleKind::Terminal && rule.has_attribute("reserved") {
            let message = format!("only terminals can be @reserved, not {}", rule.name.text);
            self.error(DiagnosticCode::RuleForm, message, rule.name.span);
        }
        if rule.transparent && kind == RuleKind::Terminal {
            let message = format!("the terminal {} cannot be transparent", rule.name.text);
            self.error(DiagnosticCode::RuleForm, message, rule.name.span);
        }
        let memo = match (rule.has_attribute("memo"), rule.has_attribute("nomemo")) {
            (true, true) => {
                let message = format!(
                    "the rule {} cannot be both @memo and @nomemo",
                    rule.name.text
                );
                self.error(DiagnosticCode::RuleForm, message, rule.name.span);
                ir::MemoMark::None
            }
            (true, false) => ir::MemoMark::Memo,
            (false, true) => ir::MemoMark::NoMemo,
            (false, false) => ir::MemoMark::None,
        };
        let params: HashMap<String, Expr> = rule
            .params
            .iter()
            .map(|p| p.text.clone())
            .zip(args)
            .collect();
        let mode = match kind {
            RuleKind::Syntactic => Mode::Syntactic,
            RuleKind::Lexical => Mode::Lexical,
            RuleKind::Terminal => Mode::Plain,
        };
        let sizes: HashMap<String, usize> = (params.iter())
            .map(|(name, arg)| (name.clone(), arg.measure().0))
            .collect();
        let scope = Scope {
            mode,
            in_field: false,
            params: &params,
            sizes: &sizes,
            levels: None,
        };
        let mut body = match &rule.body {
            RuleBody::Alternatives(alternatives) => {
                // An alias gives an alternative a node of its own, which keeps its fields.
                let unaliased_field = alternatives
                    .iter()
                    .any(|a| a.alias.is_none() && a.expr.contains_field());
                if rule.transparent && unaliased_field {
                    let message = format!(
                        "the transparent rule {} has a field outside an aliased alternative",
                        rule.name.text
                    );
                    self.error(DiagnosticCode::TransparentField, message, rule.name.span);
                }
                if kind == RuleKind::Terminal {
                    self.check_terminal_builders(rule, alternatives);
                }
                let mut lowered = Vec::new();
                for alternative in alternatives {
                    let expr = self.lower(&alternative.expr, scope);
                    if let Some(builder) = &alternative.builder {
                        let makes_node = alternative.alias.is_some() || !rule.is_underscored();
                        self.check_builder(rule, &rule.name.text, builder, makes_node, &expr);
                    }
                    lowered.push(self.wrap(rule, kind, alternative, expr));
                }
                choice(lowered)
            }
            RuleBody::Precedence(levels) => self.lower_precedence(rule, kind, levels, memo),
        };
        if kind == RuleKind::Terminal && rule.has_attribute("reserved") {
            body = ir::Expr::Unreserved(Box::new(body));
        }
        self.rules[id.0 as usize] = Some(ir::Rule {
            name: rule.name.text.clone(),
            span: rule.name.span,
            body,
            // The mark of a precedence block is for its levels, the block only calls the first.
            memo: match rule.body {
                RuleBody::Alternatives(_) => memo,
                RuleBody::Precedence(_) => ir::MemoMark::None,
            },
            level: false,
        });
    }

    /// The node an alternative of a rule produces.
    fn wrap(
        &mut self,
        rule: &Rule,
        kind: RuleKind,
        alternative: &Alternative,
        expr: ir::Expr,
    ) -> ir::Expr {
        let builder = alternative.builder.as_ref();
        if kind == RuleKind::Terminal {
            if let Some(alias) = &alternative.alias {
                let message = format!("the terminal {} cannot have an alias", rule.name.text);
                self.error(DiagnosticCode::RuleForm, message, alias.span);
            }
            return expr;
        }
        let expr = Box::new(expr);
        if let Some(alias) = &alternative.alias {
            ir::Expr::Node {
                kind: self.kind(&alias.text, &alias.text, builder, false),
                expr,
            }
        } else if rule.is_underscored() {
            *expr
        } else if rule.transparent {
            ir::Expr::Transparent {
                kind: self.kind(&rule.name.text, &rule.name.text, builder, false),
                expr,
            }
        } else {
            ir::Expr::Node {
                kind: self.kind(&rule.name.text, &rule.name.text, builder, false),
                expr,
            }
        }
    }

    /// Report a terminal whose alternatives give different builders, since every token of a
    /// terminal has the terminal's one kind.
    fn check_terminal_builders(&mut self, rule: &Rule, alternatives: &[Alternative]) {
        let texts: Vec<Option<String>> = (alternatives.iter())
            .map(|a| a.builder.as_ref().map(builder_text))
            .collect();
        if texts.windows(2).any(|pair| pair[0] != pair[1]) {
            let message = format!(
                "the alternatives of the terminal {} give different builders, and a terminal \
                 makes one kind of token",
                rule.name.text
            );
            self.error(DiagnosticCode::BuilderPlace, message, rule.name.span);
        }
    }

    fn lower_precedence(
        &mut self,
        rule: &Rule,
        kind: RuleKind,
        levels: &[Level],
        memo: ir::MemoMark,
    ) -> ir::Expr {
        if kind != RuleKind::Syntactic || rule.transparent || rule.is_underscored() {
            let message = format!(
                "the precedence block {} must be a plain syntactic rule",
                rule.name.text
            );
            self.error(DiagnosticCode::Precedence, message, rule.name.span);
        }
        // Its levels would refer to parameters that nothing binds, which only adds errors.
        if !rule.params.is_empty() {
            let message = format!(
                "the precedence block {} cannot have parameters",
                rule.name.text
            );
            self.error(DiagnosticCode::Precedence, message, rule.name.span);
            return ir::Expr::Empty;
        }
        let mut scope_names = HashMap::new();
        let mut ids = Vec::new();
        for level in levels {
            // A level whose name another level owns, or a level of a block with parameters, gets
            // a rule of its own that only its block calls.
            let id = match self.levels.get(level.name.text.as_str()) {
                Some(&(id, owner)) if std::ptr::eq(owner, level) => id,
                _ => self.allocate(),
            };
            ids.push(id);
            if self.by_name.contains_key(level.name.text.as_str()) {
                let message = format!("the level {} has the name of a rule", level.name.text);
                self.error(DiagnosticCode::Duplicate, message, level.name.span);
            }
            if scope_names.insert(level.name.text.clone(), id).is_some() {
                let message = format!("the level {} is defined twice", level.name.text);
                self.error(DiagnosticCode::Duplicate, message, level.name.span);
            }
        }
        let (no_params, no_sizes) = (HashMap::new(), HashMap::new());
        let scope = Scope {
            mode: Mode::Syntactic,
            in_field: false,
            params: &no_params,
            sizes: &no_sizes,
            levels: Some(&scope_names),
        };
        for (k, level) in levels.iter().enumerate() {
            let mut lowered = Vec::new();
            for alternative in &level.alternatives {
                let expr = self.lower(&alternative.expr, scope);
                if let Some(builder) = &alternative.builder {
                    let aliased = alternative.alias.is_some();
                    self.check_builder(rule, &level.name.text, builder, aliased, &expr);
                }
                match &alternative.alias {
                    Some(alias) => lowered.push(ir::Expr::Node {
                        kind: self.kind(
                            &alias.text,
                            &alias.text,
                            alternative.builder.as_ref(),
                            false,
                        ),
                        expr: Box::new(expr),
                    }),
                    None => {
                        if alternative.expr.contains_field() {
                            let message =
                                "an alternative with fields needs -> in a precedence block";
                            self.error(
                                DiagnosticCode::Precedence,
                                message.into(),
                                alternative.expr.span,
                            );
                        }
                        lowered.push(expr);
                    }
                }
            }
            if let Some(next) = ids.get(k + 1) {
                lowered.push(ir::Expr::Call(*next));
            }
            self.rules[ids[k].0 as usize] = Some(ir::Rule {
                name: level.name.text.clone(),
                span: level.name.span,
                body: choice(lowered),
                memo,
                level: true,
            });
        }
        match ids.first() {
            Some(first) => ir::Expr::Call(*first),
            None => {
                let message = format!("the precedence block {} has no levels", rule.name.text);
                self.error(DiagnosticCode::Precedence, message, rule.name.span);
                ir::Expr::Empty
            }
        }
    }

    fn lower(&mut self, expr: &Expr, scope: Scope<'_>) -> ir::Expr {
        self.lower_parts(expr, scope, None)
    }

    /// Lower an expression. With `parts`, also collect what one match of the enclosing field
    /// captures from: the expression itself, or the parts inside it when it is a choice, a
    /// repetition, a separated repetition, an error label or a parameter.
    fn lower_parts(
        &mut self,
        expr: &Expr,
        scope: Scope<'_>,
        mut parts: Option<&mut Vec<ir::Expr>>,
    ) -> ir::Expr {
        use ir::Expr as E;
        let lowered = match &expr.kind {
            ExprKind::Empty => E::Empty,
            ExprKind::Literal {
                text,
                case_insensitive,
            } => {
                if text.is_empty() {
                    self.error(
                        DiagnosticCode::EmptyLiteral,
                        "a literal cannot be empty, use () for the empty string".into(),
                        expr.span,
                    );
                }
                let atom = E::Literal {
                    text: text.as_str().into(),
                    case_insensitive: *case_insensitive,
                };
                self.atom(expr, atom, scope)
            }
            ExprKind::Class(class) => {
                self.check_class(&class.items, expr.span);
                let set = CharSet {
                    negated: class.negated,
                    items: class.items.clone(),
                };
                self.atom(expr, E::Class(Box::new(set)), scope)
            }
            ExprKind::Any => self.atom(expr, E::Any, scope),
            ExprKind::Reference { name, args } => {
                if let Some(arg) = scope.params.get(&name.text) {
                    if !args.is_empty() {
                        let message = format!("the parameter {} takes no arguments", name.text);
                        self.error(DiagnosticCode::Arguments, message, expr.span);
                        return E::Empty;
                    }
                    let (no_params, no_sizes) = (HashMap::new(), HashMap::new());
                    let closed = Scope {
                        params: &no_params,
                        sizes: &no_sizes,
                        levels: None,
                        ..scope
                    };
                    return self.lower_parts(arg, closed, parts);
                }
                self.reference(name, args, scope, expr.span)
            }
            ExprKind::Sequence(items) => {
                // A field captures a token only as its whole match, not next to others.
                let scope = Scope {
                    in_field: scope.in_field && items.len() == 1,
                    ..scope
                };
                E::Seq(items.iter().map(|e| self.lower(e, scope)).collect())
            }
            ExprKind::Choice(items) => {
                let items = items
                    .iter()
                    .map(|e| self.lower_parts(e, scope, parts.as_deref_mut()));
                return choice(items.collect());
            }
            ExprKind::Repeat {
                expr: inner,
                min,
                max,
            } => {
                if max.is_some_and(|max| max < *min) {
                    self.error(
                        DiagnosticCode::Bounds,
                        "a repetition's maximum is below its minimum".into(),
                        expr.span,
                    );
                }
                // A repetition of no matches captures nothing.
                let parts = parts.filter(|_| *max != Some(0));
                return E::Repeat {
                    expr: Box::new(self.lower_parts(inner, scope, parts)),
                    min: *min,
                    max: *max,
                };
            }
            ExprKind::Separated {
                item,
                separator,
                at_least_one,
            } => {
                let item = Arc::new(self.lower_parts(item, scope, parts.as_deref_mut()));
                let unfielded = Scope {
                    in_field: false,
                    ..scope
                };
                let separator = self.lower(separator, unfielded);
                if let Some(parts) = parts {
                    parts.push(separator.clone());
                }
                let tail = E::Repeat {
                    expr: Box::new(E::Seq(vec![separator, E::Shared(Arc::clone(&item))])),
                    min: 0,
                    max: None,
                };
                let some = E::Seq(vec![E::Shared(item), tail]);
                if *at_least_one {
                    return some;
                }
                return E::Repeat {
                    expr: Box::new(some),
                    min: 0,
                    max: Some(1),
                };
            }
            ExprKind::And(inner) | ExprKind::Not(inner) => {
                let unfielded = Scope {
                    in_field: false,
                    ..scope
                };
                let inner = Box::new(self.lower(inner, unfielded));
                match expr.kind {
                    ExprKind::And(_) => E::And(inner),
                    _ => E::Not(inner),
                }
            }
            ExprKind::Field { name, expr: inner } => {
                let fielded = Scope {
                    in_field: true,
                    ..scope
                };
                let mut field_parts = Vec::new();
                let inner = self.lower_parts(inner, fielded, Some(&mut field_parts));
                self.field_checks.push(FieldCheck {
                    name: name.text.clone(),
                    span: expr.span,
                    parts: field_parts,
                });
                E::Field {
                    field: self.field(name),
                    expr: Box::new(inner),
                }
            }
            ExprKind::Cut => E::Cut,
            ExprKind::Label { expr: inner, label } => {
                let label = match label {
                    ErrorLabel::Message(text) => ir::Label::Message(text.value.clone()),
                    ErrorLabel::Named(name) => ir::Label::Named(name.text.clone()),
                };
                let label = self.label(label);
                return E::Label {
                    label,
                    expr: Box::new(self.lower_parts(inner, scope, parts)),
                    skip: scope.mode == Mode::Syntactic,
                };
            }
            ExprKind::FlagOn { flag, expr: inner } => E::FlagOn {
                flag: self.flag(flag),
                expr: Box::new(self.lower(inner, scope)),
            },
            ExprKind::FlagOff { flag, expr: inner } => E::FlagOff {
                flag: self.flag(flag),
                expr: Box::new(self.lower(inner, scope)),
            },
            ExprKind::FlagTest { flag, negated } => E::FlagTest {
                flag: self.flag(flag),
                negated: *negated,
            },
            ExprKind::Indent(inner) => E::Indent {
                expr: Box::new(self.lower(inner, scope)),
                skip: scope.mode == Mode::Syntactic,
            },
            ExprKind::Same => E::Same {
                skip: scope.mode == Mode::Syntactic,
            },
            ExprKind::Def { name, expr: inner } => E::Def {
                capture: self.capture(name),
                expr: Box::new(self.lower(inner, scope)),
                skip: scope.mode == Mode::Syntactic,
            },
            ExprKind::Ref { name } => {
                // The text it matches is a token like a literal's, so the tree stays lossless.
                let atom = E::Ref {
                    capture: self.capture(name),
                };
                self.atom(expr, atom, scope)
            }
            ExprKind::Error { .. } => E::ErrorAlternative,
            ExprKind::Inherited => {
                self.error(
                    DiagnosticCode::NotYet,
                    "... needs a grammar that uses %extends (M7)".into(),
                    expr.span,
                );
                E::Empty
            }
        };
        if let Some(parts) = parts {
            parts.push(lowered.clone());
        }
        lowered
    }

    /// A literal, class or `.` in the current mode: an anonymous token outside terminals.
    fn atom(&mut self, written: &Expr, atom: ir::Expr, scope: Scope<'_>) -> ir::Expr {
        if scope.mode == Mode::Plain {
            return atom;
        }
        let name = expr_text(written);
        let kind = self.kind(&name, &name, None, true);
        let mut inner = atom;
        if let ExprKind::Literal {
            text,
            case_insensitive,
        } = &written.kind
        {
            // Word boundaries are opt-in: only a grammar that declares %word gets them.
            let boundary = self.grammar.word.is_some();
            if boundary && text.chars().last().is_some_and(|c| self.word.contains(c)) {
                inner = ir::Expr::Seq(vec![inner, ir::Expr::WordEnd]);
            }
            let is_word = !text.is_empty() && text.chars().all(|c| self.word.contains(c));
            if is_word && !self.soft.contains(text.as_str()) {
                if *case_insensitive {
                    self.reserved.folded.insert(text.to_ascii_lowercase());
                } else {
                    self.reserved.exact.insert(text.clone());
                }
            }
        }
        let token = ir::Expr::Token {
            kind,
            hidden: !scope.in_field,
            expr: Box::new(inner),
        };
        match scope.mode {
            Mode::Syntactic => ir::Expr::Seq(vec![ir::Expr::Skip, token]),
            _ => token,
        }
    }

    /// A reference to a level or a rule. References to parameters are lowered by `lower_parts`.
    fn reference(&mut self, name: &Name, args: &[Expr], scope: Scope<'_>, span: Span) -> ir::Expr {
        let level = (scope
            .levels
            .and_then(|levels| levels.get(&name.text))
            .copied())
        .or_else(|| self.levels.get(name.text.as_str()).map(|&(id, _)| id));
        if let Some(id) = level {
            if !args.is_empty() {
                self.error(
                    DiagnosticCode::Arguments,
                    format!("the level {} takes no arguments", name.text),
                    span,
                );
            }
            // A level is syntactic, which no terminal and no skip expression may use.
            if scope.mode == Mode::Plain {
                let message = format!(
                    "a terminal or the skip expression cannot use the level {}",
                    name.text
                );
                self.error(DiagnosticCode::RuleInTerminal, message, span);
                return ir::Expr::Empty;
            }
            return ir::Expr::Call(id);
        }
        let Some(&target) = self.by_name.get(name.text.as_str()) else {
            self.error(
                DiagnosticCode::UnknownRule,
                format!("unknown rule {}", name.text),
                span,
            );
            return ir::Expr::Empty;
        };
        if target.params.len() != args.len() {
            let message = format!(
                "{} takes {} arguments, {} given",
                name.text,
                target.params.len(),
                args.len()
            );
            self.error(DiagnosticCode::Arguments, message, span);
            return ir::Expr::Empty;
        }
        // Substituting would leave the arguments in place and take the name for a rule's.
        if let Some(param) = args.iter().find_map(|a| misused_parameter(a, scope.params)) {
            let message = format!("the parameter {} takes no arguments", param.text);
            self.error(DiagnosticCode::Arguments, message, param.span);
            return ir::Expr::Empty;
        }
        // Arguments that take in parameters grow with every instance, so their size is known
        // before they are built.
        let size = (args.iter())
            .map(|a| substituted_size(a, scope.sizes))
            .fold(0, usize::saturating_add);
        if size > MAX_INSTANCE_EXPRESSIONS {
            self.too_large(&target.name.text, span);
            return ir::Expr::Empty;
        }
        let closed: Vec<Expr> = args.iter().map(|a| substitute(a, scope.params)).collect();
        // An argument that takes in a parameter nests deeper with every instance.
        if closed.iter().any(|a| a.measure().1 > MAX_EXPRESSION_DEPTH) {
            let message = format!(
                "the arguments of {} nest deeper than {MAX_EXPRESSION_DEPTH} levels",
                name.text
            );
            self.error(DiagnosticCode::TooDeep, message, span);
            return ir::Expr::Empty;
        }
        // A call to a placeholder would reach a rule that is not there.
        let Some(id) = self.instance(target, closed, span) else {
            return ir::Expr::Empty;
        };
        match (scope.mode, target.kind()) {
            (Mode::Plain, RuleKind::Terminal) => ir::Expr::Call(id),
            (Mode::Plain, _) => {
                let message = format!(
                    "a terminal or the skip expression cannot use the rule {}",
                    name.text
                );
                self.error(DiagnosticCode::RuleInTerminal, message, span);
                ir::Expr::Empty
            }
            (mode, RuleKind::Terminal) => {
                let display = target
                    .display
                    .as_ref()
                    .map_or(&target.name.text, |d| &d.value);
                // The alternatives of a terminal give one builder, or an error says otherwise.
                let builder = match &target.body {
                    RuleBody::Alternatives(alternatives) => {
                        alternatives.first().and_then(|a| a.builder.as_ref())
                    }
                    RuleBody::Precedence(_) => None,
                };
                let kind = self.kind(&target.name.text, display, builder, true);
                let token = ir::Expr::Token {
                    kind,
                    hidden: target.is_underscored() && !scope.in_field,
                    expr: Box::new(ir::Expr::Call(id)),
                };
                match mode {
                    Mode::Syntactic => ir::Expr::Seq(vec![ir::Expr::Skip, token]),
                    _ => token,
                }
            }
            (Mode::Syntactic, RuleKind::Lexical) => {
                ir::Expr::Seq(vec![ir::Expr::Skip, ir::Expr::Call(id)])
            }
            _ => ir::Expr::Call(id),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use super::resolve;
    use crate::grammar::builder::{GrammarBuilder, lit, r, sep_many};
    use crate::grammar::print::builder_text;
    use crate::ir;
    use crate::reference::Reference;
    use crate::{Grammar, Parser};

    /// The expressions of all rules, counting a shared one once.
    fn size(grammar: &ir::Grammar) -> usize {
        let mut seen = HashSet::new();
        let mut pending: Vec<&ir::Expr> = grammar.rules.iter().map(|r| &r.body).collect();
        let mut size = 0;
        while let Some(expr) = pending.pop() {
            size += 1;
            if let ir::Expr::Shared(inner) = expr
                && !seen.insert(Arc::as_ptr(inner))
            {
                continue;
            }
            pending.extend(expr.children());
        }
        size
    }

    #[test]
    fn nested_separated_repetitions_cost_linear_time_and_space() {
        let mut expr = lit("a");
        for _ in 0..40 {
            expr = sep_many(expr, lit(","));
        }
        let mut g = GrammarBuilder::new();
        // In a rule without a node of its own, the field check walks the whole body.
        g.rule("start", r("_items"));
        g.rule("_items", expr);
        let grammar = g.build();
        let resolved = resolve(&grammar).expect("the grammar resolves");
        // Forty levels that each copied their item would hold more than a trillion.
        assert!(size(&resolved) < 1_000, "{}", size(&resolved));
        let reference = Reference::new(&grammar).expect("the grammar resolves");
        assert!(format!("{reference:?}").len() < 100);
    }

    #[test]
    fn an_alias_named_like_a_terminal_makes_nodes_of_a_kind_apart() {
        let text = "start: x NUM\nx: \"a\" -> NUM\nNUM: \"b\"";
        let grammar = Grammar::from_text(text).unwrap_or_else(|e| panic!("{e:?}"));
        let parser = Parser::new(&grammar).unwrap_or_else(|e| panic!("{e:?}"));
        let mut tokens: Vec<bool> = (parser.kinds().iter())
            .filter(|kind| kind.name == "NUM")
            .map(|kind| kind.token)
            .collect();
        tokens.sort();
        assert_eq!(tokens, [false, true]);
    }

    #[test]
    fn each_builder_has_a_kind_of_its_own_named_like_its_rule() {
        let text = "?value: list | NUM | \"true\" => True | \"false\" => False
                    list: \"[\" (value ** \",\") \"]\" => list
                    row: () -> list => list
                    NUM: [0-9]+ => int
                    %ignore \" \"+";
        let grammar = Grammar::from_text(text).unwrap_or_else(|e| panic!("{e:?}"));
        let parser = Parser::new(&grammar).unwrap_or_else(|e| panic!("{e:?}"));
        let builders = |name: &str| -> Vec<Option<String>> {
            (parser.kinds().iter())
                .filter(|kind| kind.name == name)
                .map(|kind| kind.builder.as_ref().map(builder_text))
                .collect()
        };
        let some = |text: &str| Some(text.to_owned());
        assert_eq!(builders("value"), [None, some("True"), some("False")]);
        // Equal builders share a kind, also across rules through an alias.
        assert_eq!(builders("list"), [some("list")]);
        assert_eq!(builders("NUM"), [some("int")]);
        // The tree reads as it would without builders.
        let input = "[1, true]";
        let tree = parser
            .parse(input, "value")
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            tree.to_text(input, false),
            "(list ~\"[\" NUM\"1\" ~\",\" (value ~\"true\") ~\"]\")"
        );
    }

    #[test]
    fn a_transparent_rule_that_always_leaves_one_item_makes_no_node() {
        let text = "start: (statement ** \";\")
                    ?statement: show | NAME
                    show: \"show\" atom
                    ?atom: NAME | \"(\" atom \")\" | name=HEX -> hex
                    ?pair: atom atom?
                    NAME: [a-z]+
                    HEX: \"0x\" DIGIT+
                    DIGIT: [0-9]";
        let grammar = Grammar::from_text(text).unwrap_or_else(|e| panic!("{e:?}"));
        let parser = Parser::new(&grammar).unwrap_or_else(|e| panic!("{e:?}"));
        let made = |name: &str| -> Vec<bool> {
            (parser.kinds().iter())
                .filter(|kind| kind.name == name)
                .map(|kind| kind.made)
                .collect()
        };
        assert_eq!(made("statement"), [false]);
        assert_eq!(made("atom"), [false]);
        // An alias always makes a node, and one item or two keeps a transparent node sometimes.
        assert_eq!(made("hex"), [true]);
        assert_eq!(made("pair"), [true]);
        // A terminal only other terminals use makes tokens when a parse starts from it.
        assert_eq!(made("DIGIT"), [true]);
        assert_eq!(made("<error>"), [false]);
    }
}
