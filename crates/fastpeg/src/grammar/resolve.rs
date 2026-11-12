// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! Resolve a grammar: check its names and translate it into the core language, following the
//! translation steps of the semantics page.

#![cfg_attr(
    not(feature = "reference"),
    expect(
        dead_code,
        reason = "only the reference interpreter resolves grammars until the engine exists"
    )
)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::Arc;

use super::print::expr_text;
use super::{ClassItem, ErrorLabel, Expr, ExprKind, Grammar, Name, Rule, RuleBody, RuleKind, Span};
use crate::ir::{self, CaptureId, CharSet, FlagId, LabelId, RuleId};
use crate::tree::{FieldId, KindId, Names};

/// Template instances resolving may create before it gives up, so that a template whose
/// arguments grow with every use cannot make it run forever.
const MAX_RULE_INSTANCES: usize = 10_000;

/// The tab width when a grammar sets none.
const DEFAULT_TAB_WIDTH: u32 = 8;

/// The attributes a rule may carry.
const ATTRIBUTES: [&str; 5] = ["lexical", "reserved", "sync", "memo", "nomemo"];

/// A problem that makes a grammar unusable.
#[derive(Clone, Debug, PartialEq)]
pub struct GrammarError {
    /// What is wrong.
    pub message: String,
    /// Where in the grammar source. Empty for grammars built in code.
    pub span: Span,
}

impl fmt::Display for GrammarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for GrammarError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Syntactic,
    Lexical,
    Plain,
}

/// Where an expression is lowered: the mode, whether it stands directly inside a field, the
/// arguments of the template instance, and the level names of a precedence block.
#[derive(Clone, Copy)]
struct Scope<'s> {
    mode: Mode,
    in_field: bool,
    params: &'s HashMap<String, Expr>,
    levels: Option<&'s HashMap<String, RuleId>>,
}

struct Pending<'g> {
    id: RuleId,
    rule: &'g Rule,
    args: Vec<Expr>,
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
    rules: Vec<Option<ir::Rule>>,
    instances: HashMap<(String, String), RuleId>,
    queue: VecDeque<Pending<'g>>,
    kinds: Interner,
    kind_display: Vec<String>,
    fields: Interner,
    flags: HashMap<&'g str, FlagId>,
    captures: Interner,
    labels: Vec<ir::Label>,
    label_ids: HashMap<ir::Label, LabelId>,
    word: CharSet,
    soft: HashSet<&'g str>,
    reserved: ir::Reserved,
    errors: Vec<GrammarError>,
}

/// Resolve a grammar into the core language, or report every problem found.
pub(crate) fn resolve(grammar: &Grammar) -> Result<ir::Grammar, Vec<GrammarError>> {
    let mut r = Resolver {
        grammar,
        by_name: HashMap::new(),
        rules: Vec::new(),
        instances: HashMap::new(),
        queue: VecDeque::new(),
        kinds: Interner::with(&["<error>", "<missing>", "<skipped>"]),
        kind_display: vec!["error".into(), "missing".into(), "skipped".into()],
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
        errors: Vec::new(),
    };
    r.directives();
    for rule in &grammar.rules {
        r.check_name(&rule.name);
        if r.by_name.insert(&rule.name.text, rule).is_some() {
            r.error(
                format!("the rule {} is defined twice", rule.name.text),
                rule.name.span,
            );
        }
    }
    let mut entry = HashMap::new();
    for rule in &grammar.rules {
        if rule.params.is_empty() {
            r.instance(rule, Vec::new(), rule.name.span);
        }
    }
    while let Some(pending) = r.queue.pop_front() {
        r.lower_rule(pending);
    }
    let no_params = HashMap::new();
    let plain = Scope {
        mode: Mode::Plain,
        in_field: false,
        params: &no_params,
        levels: None,
    };
    let skip = match grammar.ignore.as_slice() {
        [] => None,
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
    while let Some(pending) = r.queue.pop_front() {
        r.lower_rule(pending);
    }
    if !r.errors.is_empty() {
        return Err(r.errors);
    }
    let rules = r.rules.into_iter().flatten().collect();
    Ok(ir::Grammar {
        rules,
        entry,
        names: Arc::new(Names {
            kinds: r.kinds.names,
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
    fn error(&mut self, message: String, span: Span) {
        self.errors.push(GrammarError { message, span });
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
            self.error(message, name.span);
        }
    }

    fn directives(&mut self) {
        let g = self.grammar;
        for flag in &g.flags {
            let next = FlagId(self.flags.len() as u16);
            self.flags.entry(&flag.text).or_insert(next);
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
                _ => self.error("%word needs a character class".into(), word.span),
            }
        }
        if let Some(tabs) = g.tabs
            && tabs.value == 0
        {
            self.error("%tabs needs a width of at least 1".into(), tabs.span);
        }
        if let Some(module) = &g.extends {
            self.error(
                "%extends is not supported yet, see the roadmap (M7)".into(),
                module.span,
            );
        }
        for import in &g.imports {
            let message = "%import is not supported yet, see the roadmap (M7)".into();
            self.error(message, import.module.span);
        }
        for recover in &g.recovers {
            let message = "%recover is not supported yet, see the roadmap (M8)".into();
            self.error(message, recover.label.span);
        }
    }

    /// The id of the instance of `rule` with these closed arguments, created on first use.
    fn instance(&mut self, rule: &'g Rule, args: Vec<Expr>, span: Span) -> RuleId {
        let key: Vec<String> = args.iter().map(expr_text).collect();
        let key = (rule.name.text.clone(), key.join("\u{1}"));
        if let Some(&id) = self.instances.get(&key) {
            return id;
        }
        if self.rules.len() >= MAX_RULE_INSTANCES {
            let message = format!("the template {} creates too many instances", rule.name.text);
            if !self.errors.iter().any(|e| e.message == message) {
                self.error(message, span);
            }
            return RuleId(0);
        }
        let id = self.allocate();
        self.instances.insert(key, id);
        self.queue.push_back(Pending { id, rule, args });
        id
    }

    fn allocate(&mut self) -> RuleId {
        self.rules.push(None);
        RuleId((self.rules.len() - 1) as u32)
    }

    fn kind(&mut self, name: &str, display: &str) -> KindId {
        let (id, new) = self.kinds.id(name);
        if new {
            self.kind_display.push(display.to_owned());
        }
        KindId(id)
    }

    fn field(&mut self, name: &Name) -> FieldId {
        let (id, _) = self.fields.id(&name.text);
        match u16::try_from(id) {
            Ok(id) => FieldId(id),
            Err(_) => {
                self.error("the grammar has too many field names".into(), name.span);
                FieldId::NONE
            }
        }
    }

    fn capture(&mut self, name: &Name) -> CaptureId {
        let (id, _) = self.captures.id(&name.text);
        CaptureId(u16::try_from(id).unwrap_or(u16::MAX))
    }

    fn flag(&mut self, name: &Name) -> FlagId {
        match self.flags.get(name.text.as_str()) {
            Some(&id) => id,
            None => {
                let message = format!("the flag {} is not declared with %flag", name.text);
                self.error(message, name.span);
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
            self.error("a character class must list something".into(), span);
        }
        for item in items {
            if let ClassItem::Range(first, last) = item
                && first > last
            {
                let message = format!("the range {first:?}-{last:?} runs backwards");
                self.error(message, span);
            }
        }
    }

    fn lower_rule(&mut self, pending: Pending<'g>) {
        let Pending { id, rule, args } = pending;
        let kind = rule.kind();
        for attribute in &rule.attributes {
            if !ATTRIBUTES.contains(&attribute.text.as_str()) {
                self.error(
                    format!("unknown attribute @{}", attribute.text),
                    attribute.span,
                );
            }
        }
        if kind == RuleKind::Terminal && rule.has_attribute("lexical") {
            let message = format!("the terminal {} cannot be @lexical", rule.name.text);
            self.error(message, rule.name.span);
        }
        if kind != RuleKind::Terminal && rule.has_attribute("reserved") {
            let message = format!("only terminals can be @reserved, not {}", rule.name.text);
            self.error(message, rule.name.span);
        }
        if rule.transparent && kind == RuleKind::Terminal {
            let message = format!("the terminal {} cannot be transparent", rule.name.text);
            self.error(message, rule.name.span);
        }
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
        let scope = Scope {
            mode,
            in_field: false,
            params: &params,
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
                    self.error(message, rule.name.span);
                }
                let mut lowered = Vec::new();
                for alternative in alternatives {
                    let expr = self.lower(&alternative.expr, scope);
                    lowered.push(self.wrap(rule, kind, alternative.alias.as_ref(), expr));
                }
                choice(lowered)
            }
            RuleBody::Precedence(levels) => self.lower_precedence(rule, kind, levels),
        };
        if kind == RuleKind::Terminal && rule.has_attribute("reserved") {
            body = ir::Expr::Unreserved(Box::new(body));
        }
        self.rules[id.0 as usize] = Some(ir::Rule { body });
    }

    /// The node an alternative of a rule produces.
    fn wrap(
        &mut self,
        rule: &Rule,
        kind: RuleKind,
        alias: Option<&Name>,
        expr: ir::Expr,
    ) -> ir::Expr {
        if kind == RuleKind::Terminal {
            if let Some(alias) = alias {
                let message = format!("the terminal {} cannot have an alias", rule.name.text);
                self.error(message, alias.span);
            }
            return expr;
        }
        let expr = Box::new(expr);
        if let Some(alias) = alias {
            ir::Expr::Node {
                kind: self.kind(&alias.text, &alias.text),
                expr,
            }
        } else if rule.is_underscored() {
            *expr
        } else if rule.transparent {
            ir::Expr::Transparent {
                kind: self.kind(&rule.name.text, &rule.name.text),
                expr,
            }
        } else {
            ir::Expr::Node {
                kind: self.kind(&rule.name.text, &rule.name.text),
                expr,
            }
        }
    }

    fn lower_precedence(
        &mut self,
        rule: &Rule,
        kind: RuleKind,
        levels: &[super::Level],
    ) -> ir::Expr {
        if kind != RuleKind::Syntactic || rule.transparent || rule.is_underscored() {
            let message = format!(
                "the precedence block {} must be a plain syntactic rule",
                rule.name.text
            );
            self.error(message, rule.name.span);
        }
        if !rule.params.is_empty() {
            let message = format!(
                "the precedence block {} cannot have parameters",
                rule.name.text
            );
            self.error(message, rule.name.span);
        }
        let mut scope_names = HashMap::new();
        let mut ids = Vec::new();
        for level in levels {
            let id = self.allocate();
            ids.push(id);
            if self.by_name.contains_key(level.name.text.as_str()) {
                let message = format!("the level {} shadows a rule", level.name.text);
                self.error(message, level.name.span);
            }
            if scope_names.insert(level.name.text.clone(), id).is_some() {
                let message = format!("the level {} is defined twice", level.name.text);
                self.error(message, level.name.span);
            }
        }
        let no_params = HashMap::new();
        let scope = Scope {
            mode: Mode::Syntactic,
            in_field: false,
            params: &no_params,
            levels: Some(&scope_names),
        };
        for (k, level) in levels.iter().enumerate() {
            let mut lowered = Vec::new();
            for alternative in &level.alternatives {
                let expr = self.lower(&alternative.expr, scope);
                match &alternative.alias {
                    Some(alias) => lowered.push(ir::Expr::Node {
                        kind: self.kind(&alias.text, &alias.text),
                        expr: Box::new(expr),
                    }),
                    None => {
                        if alternative.expr.contains_field() {
                            let message =
                                "an alternative with fields needs -> in a precedence block";
                            self.error(message.into(), alternative.expr.span);
                        }
                        lowered.push(expr);
                    }
                }
            }
            if let Some(next) = ids.get(k + 1) {
                lowered.push(ir::Expr::Call(*next));
            }
            self.rules[ids[k].0 as usize] = Some(ir::Rule {
                body: choice(lowered),
            });
        }
        match ids.first() {
            Some(first) => ir::Expr::Call(*first),
            None => {
                let message = format!("the precedence block {} has no levels", rule.name.text);
                self.error(message, rule.name.span);
                ir::Expr::Empty
            }
        }
    }

    fn lower(&mut self, expr: &Expr, scope: Scope<'_>) -> ir::Expr {
        use ir::Expr as E;
        match &expr.kind {
            ExprKind::Empty => E::Empty,
            ExprKind::Literal {
                text,
                case_insensitive,
            } => {
                if text.is_empty() {
                    self.error(
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
            ExprKind::Reference { name, args } => self.reference(name, args, scope, expr.span),
            ExprKind::Sequence(items) => {
                // A field captures a token only as its whole match, not next to others.
                let scope = Scope {
                    in_field: scope.in_field && items.len() == 1,
                    ..scope
                };
                E::Seq(items.iter().map(|e| self.lower(e, scope)).collect())
            }
            ExprKind::Choice(items) => choice(items.iter().map(|e| self.lower(e, scope)).collect()),
            ExprKind::Repeat {
                expr: inner,
                min,
                max,
            } => {
                if max.is_some_and(|max| max < *min) {
                    self.error(
                        "a repetition's maximum is below its minimum".into(),
                        expr.span,
                    );
                }
                E::Repeat {
                    expr: Box::new(self.lower(inner, scope)),
                    min: *min,
                    max: *max,
                }
            }
            ExprKind::Separated {
                item,
                separator,
                at_least_one,
            } => {
                let item = self.lower(item, scope);
                let unfielded = Scope {
                    in_field: false,
                    ..scope
                };
                let separator = self.lower(separator, unfielded);
                let tail = E::Repeat {
                    expr: Box::new(E::Seq(vec![separator, item.clone()])),
                    min: 0,
                    max: None,
                };
                let some = E::Seq(vec![item, tail]);
                if *at_least_one {
                    some
                } else {
                    E::Repeat {
                        expr: Box::new(some),
                        min: 0,
                        max: Some(1),
                    }
                }
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
            ExprKind::Field { name, expr: inner } => E::Field {
                field: self.field(name),
                expr: Box::new(self.lower(
                    inner,
                    Scope {
                        in_field: true,
                        ..scope
                    },
                )),
            },
            ExprKind::Cut => E::Cut,
            ExprKind::Label { expr: inner, label } => {
                let label = match label {
                    ErrorLabel::Message(text) => ir::Label::Message(text.value.clone()),
                    ErrorLabel::Named(name) => ir::Label::Named(name.text.clone()),
                };
                let label = self.label(label);
                E::Choice(vec![self.lower(inner, scope), E::Throw(label)])
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
                    "... needs a grammar that uses %extends (M7)".into(),
                    expr.span,
                );
                E::Empty
            }
        }
    }

    /// A literal, class or `.` in the current mode: an anonymous token outside terminals.
    fn atom(&mut self, written: &Expr, atom: ir::Expr, scope: Scope<'_>) -> ir::Expr {
        if scope.mode == Mode::Plain {
            return atom;
        }
        let name = expr_text(written);
        let kind = self.kind(&name, &name);
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

    fn reference(&mut self, name: &Name, args: &[Expr], scope: Scope<'_>, span: Span) -> ir::Expr {
        if let Some(arg) = scope.params.get(&name.text) {
            if !args.is_empty() {
                self.error(
                    format!("the parameter {} takes no arguments", name.text),
                    span,
                );
            }
            let no_params = HashMap::new();
            let closed = Scope {
                params: &no_params,
                levels: None,
                ..scope
            };
            return self.lower(arg, closed);
        }
        if let Some(&id) = scope.levels.and_then(|levels| levels.get(&name.text)) {
            if !args.is_empty() {
                self.error(format!("the level {} takes no arguments", name.text), span);
            }
            return ir::Expr::Call(id);
        }
        let Some(&target) = self.by_name.get(name.text.as_str()) else {
            self.error(format!("unknown rule {}", name.text), span);
            return ir::Expr::Empty;
        };
        if target.params.len() != args.len() {
            let message = format!(
                "{} takes {} arguments, {} given",
                name.text,
                target.params.len(),
                args.len()
            );
            self.error(message, span);
            return ir::Expr::Empty;
        }
        let closed = args.iter().map(|a| substitute(a, scope.params)).collect();
        let id = self.instance(target, closed, span);
        match (scope.mode, target.kind()) {
            (Mode::Plain, RuleKind::Terminal) => ir::Expr::Call(id),
            (Mode::Plain, _) => {
                let message = format!(
                    "a terminal or the skip expression cannot use the rule {}",
                    name.text
                );
                self.error(message, span);
                ir::Expr::Empty
            }
            (mode, RuleKind::Terminal) => {
                let display = target
                    .display
                    .as_ref()
                    .map_or(&target.name.text, |d| &d.value);
                let kind = self.kind(&target.name.text, display);
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
