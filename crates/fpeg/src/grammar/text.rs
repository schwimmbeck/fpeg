// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The text front end: parse grammar text with the grammar of grammars on the parsing machine
//! and turn the tree into the grammar model.

use std::sync::OnceLock;

use super::{
    Alternative, BuilderArg, BuilderValue, Class, ClassItem, Constant, Count, ErrorLabel, Expr,
    ExprKind, Grammar, Import, Level, MAX_EXPRESSION_DEPTH, Name, Property, Recover, Rule,
    RuleBody, Span, Text, ValueBuilder, meta,
};
use crate::diag::{Diagnostic, DiagnosticCode};
use crate::parse::{Limit, ParseErrorKind, Recovery};
use crate::resolve::MAX_ERRORS;
use crate::tree::NodeRef;
use crate::vm::Parser;

impl Grammar {
    /// Read a grammar from its text.
    pub fn from_text(text: &str) -> Result<Grammar, Vec<Diagnostic>> {
        Grammar::read_module(text, 0)
    }

    /// Read the grammar of a module, its spans counted from `base`.
    pub(crate) fn read_module(text: &str, base: u32) -> Result<Grammar, Vec<Diagnostic>> {
        static META: OnceLock<Result<Parser, Vec<Diagnostic>>> = OnceLock::new();
        let meta = META
            .get_or_init(|| Parser::new(&meta::grammar()))
            .as_ref()
            .map_err(Clone::clone)?;
        read(meta, text, base)
    }
}

/// The label of the grammar of grammars for a separator that does not come after `**` or `++`,
/// whose error says in words what must follow, since anything could start the separator.
const SEPARATOR_LABEL: &str = "separator";

/// A syntax error of a grammar text at a position, with what could have followed there.
fn syntax_error(position: usize, expected: &[String], base: u32) -> Diagnostic {
    let at = position as u32 + base;
    Diagnostic {
        code: DiagnosticCode::Syntax,
        message: format!("expected {}", expected.join(", ")),
        span: Span { start: at, end: at },
    }
}

/// The syntax error of a mistake that reading recovered from.
fn recovered_error(recovery: &Recovery, base: u32) -> Diagnostic {
    if recovery.label == SEPARATOR_LABEL {
        let at = recovery.position as u32 + base;
        return Diagnostic {
            code: DiagnosticCode::Syntax,
            message: "expected a separator after ** or ++".to_owned(),
            span: Span { start: at, end: at },
        };
    }
    syntax_error(recovery.position, &recovery.expected, base)
}

/// Read a grammar from its text with `meta`, a parser of the grammar of grammars, its spans
/// counted from `base`, where a module's text lies among the spans of a grammar that uses it.
/// The parse recovers at the next rule or directive, so that each mistake is reported, up to the
/// number of errors that resolving reports too.
pub(super) fn read(meta: &Parser, text: &str, base: u32) -> Result<Grammar, Vec<Diagnostic>> {
    let recovered = |recoveries: &[Recovery]| -> Vec<Diagnostic> {
        (recoveries.iter().take(MAX_ERRORS))
            .map(|recovery| recovered_error(recovery, base))
            .collect()
    };
    let tree = match meta.parse_recovering(text, "start") {
        Ok(tree) if tree.recoveries().is_empty() => tree,
        Ok(tree) => return Err(recovered(tree.recoveries())),
        Err(error) => {
            let mut found = recovered(&error.recoveries);
            let at = error.position as u32 + base;
            let (code, message) = match error.kind {
                ParseErrorKind::NoMatch { expected }
                | ParseErrorKind::Label {
                    named: true,
                    expected,
                    ..
                } => (
                    DiagnosticCode::Syntax,
                    format!("expected {}", expected.join(", ")),
                ),
                // The grammar of grammars says in words what must follow, such as a separator.
                ParseErrorKind::Label { label, .. } => {
                    (DiagnosticCode::Syntax, format!("expected {label}"))
                }
                ParseErrorKind::Limit(Limit::Depth) => (
                    DiagnosticCode::TooDeep,
                    "the grammar text nests too deeply to read".to_owned(),
                ),
                ParseErrorKind::Limit(limit) => {
                    let what = match limit {
                        Limit::InputSize => "is too large",
                        Limit::Memory => "takes too much memory",
                        _ => "takes too long",
                    };
                    let message = format!("the grammar text {what} to read");
                    (DiagnosticCode::TooLarge, message)
                }
                other => (DiagnosticCode::Internal, format!("{other:?}")),
            };
            found.push(Diagnostic {
                code,
                message,
                span: Span { start: at, end: at },
            });
            return Err(found);
        }
    };
    let mut converter = Converter {
        text,
        base,
        depth: 0,
        errors: Vec::new(),
    };
    let mut grammar = Grammar::default();
    for root in tree.roots() {
        for item in visible(&root) {
            converter.item(&mut grammar, &item);
        }
    }
    for rule in &grammar.rules {
        if (rule.body.alternatives()).any(|a| a.expr.measure().1 > MAX_EXPRESSION_DEPTH) {
            converter.too_deep(rule.span);
        }
    }
    if converter.errors.is_empty() {
        Ok(grammar)
    } else {
        Err(converter.errors)
    }
}

fn visible<'t>(node: &NodeRef<'t>) -> Vec<NodeRef<'t>> {
    node.children()
        .into_iter()
        .filter(|c| !c.is_hidden())
        .collect()
}

fn field<'t>(node: &NodeRef<'t>, name: &str) -> Option<NodeRef<'t>> {
    fields(node, name).into_iter().next()
}

fn fields<'t>(node: &NodeRef<'t>, name: &str) -> Vec<NodeRef<'t>> {
    visible(node)
        .into_iter()
        .filter(|c| c.field_name() == Some(name))
        .collect()
}

struct Converter<'t> {
    text: &'t str,
    /// Where the text lies among the spans of the grammar.
    base: u32,
    /// How many choices, lookaheads and suffixes enclose the node being converted.
    depth: usize,
    errors: Vec<Diagnostic>,
}

impl<'t> Converter<'t> {
    fn span(&self, node: &NodeRef<'_>) -> Span {
        let (start, end) = node.span();
        Span {
            start: start + self.base,
            end: end + self.base,
        }
    }

    fn error(&mut self, code: DiagnosticCode, message: String, span: Span) {
        self.errors.push(Diagnostic {
            code,
            message,
            span,
        });
    }

    fn too_deep(&mut self, span: Span) {
        let message = format!("an expression nests deeper than {MAX_EXPRESSION_DEPTH} levels");
        if !self.errors.iter().any(|e| e.message == message) {
            self.error(DiagnosticCode::TooDeep, message, span);
        }
    }

    /// Go one level deeper into an expression, unless that is too deep to convert safely.
    fn enter(&mut self, node: &NodeRef<'_>, levels: usize) -> bool {
        if self.depth + levels > MAX_EXPRESSION_DEPTH {
            self.too_deep(self.span(node));
            return false;
        }
        self.depth += levels;
        true
    }

    fn item(&mut self, grammar: &mut Grammar, node: &NodeRef<'_>) {
        match node.kind_name() {
            "rule" => {
                let rule = self.rule(node);
                grammar.rules.push(rule);
            }
            "ignore" => {
                let expr = self.field_choice(node, "expr");
                grammar.ignore.push(expr);
            }
            "word" if grammar.word.is_some() => {
                let message = "a grammar says its word characters once".to_owned();
                self.error(DiagnosticCode::Duplicate, message, self.span(node));
            }
            "word" => grammar.word = Some(self.field_choice(node, "expr")),
            "flags" => {
                for name in fields(node, "names") {
                    grammar.flags.push(self.name(&name));
                }
            }
            "tabs" if grammar.tabs.is_some() => {
                let message = "a grammar says its tab width once".to_owned();
                self.error(DiagnosticCode::Duplicate, message, self.span(node));
            }
            "tabs" => grammar.tabs = field(node, "width").map(|w| self.count(&w)),
            "keywords" => {
                for word in fields(node, "words") {
                    grammar.soft_keywords.push(self.string(&word));
                }
            }
            "import" => {
                let whole = self.span(node);
                let module = field(node, "module").map_or_else(
                    || Text {
                        value: String::new(),
                        span: whole,
                    },
                    |m| self.module(&m),
                );
                let names = field(node, "names")
                    .map(|n| fields(&n, "items").iter().map(|i| self.name(i)).collect())
                    .unwrap_or_default();
                let prefix = field(node, "prefix")
                    .and_then(|p| field(&p, "name"))
                    .map(|n| self.name(&n));
                grammar.imports.push(Import {
                    module,
                    names,
                    prefix,
                });
            }
            "extends" if grammar.extends.is_some() => {
                let message = "a grammar extends at most one other".to_owned();
                self.error(DiagnosticCode::Duplicate, message, self.span(node));
            }
            "extends" => grammar.extends = field(node, "module").map(|m| self.string(&m)),
            "recover" => {
                let label = field(node, "label").map(|l| self.name(&l));
                let expr = self.field_choice(node, "expr");
                if let Some(label) = label {
                    grammar.recovers.push(Recover { label, expr });
                }
            }
            other => self.error(
                DiagnosticCode::Internal,
                format!("unexpected {other}"),
                self.span(node),
            ),
        }
    }

    fn text_of(&self, node: &NodeRef<'_>) -> &'t str {
        node.text(self.text)
    }

    fn name(&self, node: &NodeRef<'_>) -> Name {
        Name {
            text: self.text_of(node).to_owned(),
            span: self.span(node),
        }
    }

    fn field_name(&mut self, node: &NodeRef<'_>, name: &str) -> Name {
        match field(node, name) {
            Some(n) => self.name(&n),
            None => Name {
                text: String::new(),
                span: self.span(node),
            },
        }
    }

    fn count(&mut self, node: &NodeRef<'_>) -> Count {
        let value = self.text_of(node).parse().unwrap_or_else(|_| {
            let message = "the number is too large".into();
            self.error(DiagnosticCode::Number, message, self.span(node));
            0
        });
        Count {
            value,
            span: self.span(node),
        }
    }

    fn module(&mut self, node: &NodeRef<'_>) -> Text {
        if node.kind_name() == "STRING" {
            self.string(node)
        } else {
            Text {
                value: self.text_of(node).to_owned(),
                span: self.span(node),
            }
        }
    }

    fn rule(&mut self, node: &NodeRef<'_>) -> Rule {
        let attributes = fields(node, "attributes")
            .iter()
            .map(|a| self.field_name(a, "name"))
            .collect();
        let params = field(node, "params")
            .map(|p| fields(&p, "names").iter().map(|n| self.name(n)).collect())
            .unwrap_or_default();
        let display = field(node, "display").map(|d| self.string(&d));
        let body = match field(node, "body") {
            Some(body) if body.kind_name() == "precedence" => RuleBody::Precedence(
                fields(&body, "levels")
                    .iter()
                    .map(|level| Level {
                        name: self.field_name(level, "name"),
                        alternatives: field(level, "body")
                            .map(|b| self.alternatives(&b))
                            .unwrap_or_default(),
                    })
                    .collect(),
            ),
            Some(body) => RuleBody::Alternatives(self.alternatives(&body)),
            None => RuleBody::Alternatives(Vec::new()),
        };
        Rule {
            name: self.field_name(node, "name"),
            transparent: field(node, "transparent").is_some(),
            params,
            display,
            attributes,
            body,
            span: self.span(node),
        }
    }

    fn alternatives(&mut self, node: &NodeRef<'_>) -> Vec<Alternative> {
        fields(node, "items")
            .iter()
            .map(|alternative| Alternative {
                expr: field(alternative, "expr")
                    .map_or_else(|| Expr::new(ExprKind::Empty), |s| self.sequence(&s)),
                alias: field(alternative, "alias").map(|a| self.field_name(&a, "name")),
                builder: field(alternative, "builder")
                    .and_then(|b| field(&b, "value"))
                    .map(|v| self.builder(&v)),
            })
            .collect()
    }

    fn field_choice(&mut self, node: &NodeRef<'_>, name: &str) -> Expr {
        match field(node, name) {
            Some(choice) => self.choice(&choice),
            None => Expr::new(ExprKind::Empty),
        }
    }

    fn choice(&mut self, node: &NodeRef<'_>) -> Expr {
        if !self.enter(node, 1) {
            return Expr::new(ExprKind::Empty);
        }
        let mut items: Vec<Expr> = fields(node, "items")
            .iter()
            .map(|s| self.sequence(s))
            .collect();
        self.depth -= 1;
        if items.len() == 1 {
            return items.remove(0);
        }
        Expr {
            kind: ExprKind::Choice(items),
            span: self.span(node),
        }
    }

    fn sequence(&mut self, node: &NodeRef<'_>) -> Expr {
        let mut items: Vec<Expr> = fields(node, "items")
            .iter()
            .map(|e| self.element(e))
            .collect();
        if items.len() == 1 {
            return items.remove(0);
        }
        Expr {
            kind: ExprKind::Sequence(items),
            span: self.span(node),
        }
    }

    fn element(&mut self, node: &NodeRef<'_>) -> Expr {
        let expr = match field(node, "expr") {
            Some(prefix) => self.prefix(&prefix),
            None => Expr::new(ExprKind::Empty),
        };
        let Some(value) = field(node, "label").and_then(|l| field(&l, "value")) else {
            return expr;
        };
        let label = if value.kind_name() == "STRING" {
            ErrorLabel::Message(self.string(&value))
        } else {
            ErrorLabel::Named(self.name(&value))
        };
        Expr {
            kind: ExprKind::Label {
                expr: Box::new(expr),
                label,
            },
            span: self.span(node),
        }
    }

    fn prefix(&mut self, node: &NodeRef<'_>) -> Expr {
        let kind = match node.kind_name() {
            "lookahead" => {
                if !self.enter(node, 1) {
                    return Expr::new(ExprKind::Empty);
                }
                let inner = match field(node, "expr") {
                    Some(e) => self.prefix(&e),
                    None => Expr::new(ExprKind::Empty),
                };
                self.depth -= 1;
                let negative = field(node, "op").is_some_and(|op| self.text_of(&op) == "!");
                if negative {
                    ExprKind::Not(Box::new(inner))
                } else {
                    ExprKind::And(Box::new(inner))
                }
            }
            "field" => ExprKind::Field {
                name: self.field_name(node, "name"),
                expr: Box::new(match field(node, "expr") {
                    Some(e) => self.suffix(&e),
                    None => Expr::new(ExprKind::Empty),
                }),
            },
            _ => return self.suffix(node),
        };
        Expr {
            kind,
            span: self.span(node),
        }
    }

    fn suffix(&mut self, node: &NodeRef<'_>) -> Expr {
        let ops = fields(node, "ops");
        // Each suffix wraps what is before it, one level deeper.
        if !self.enter(node, ops.len()) {
            return Expr::new(ExprKind::Empty);
        }
        let mut expr = match field(node, "base") {
            Some(base) => self.primary(&base),
            None => Expr::new(ExprKind::Empty),
        };
        self.depth -= ops.len();
        for op in ops {
            let kind = match op.kind_name() {
                "optional" => repeat(expr, 0, Some(1)),
                "many" => repeat(expr, 0, None),
                "some" => repeat(expr, 1, None),
                "bounds" => {
                    let min = field(&op, "min").map_or(0, |m| self.count(&m).value);
                    let max = field(&op, "max").map(|m| self.count(&m).value);
                    match (field(&op, "comma").is_some(), max) {
                        (false, _) => repeat(expr, min, Some(min)),
                        (true, max) => repeat(expr, min, max),
                    }
                }
                _ => ExprKind::Separated {
                    item: Box::new(expr),
                    separator: Box::new(match field(&op, "separator") {
                        Some(s) => self.primary(&s),
                        None => Expr::new(ExprKind::Empty),
                    }),
                    at_least_one: field(&op, "op").is_some_and(|o| self.text_of(&o) == "++"),
                },
            };
            let (start, _) = node.span();
            let (_, end) = op.span();
            expr = Expr {
                kind,
                span: Span {
                    start: start + self.base,
                    end: end + self.base,
                },
            };
        }
        expr
    }

    fn primary(&mut self, node: &NodeRef<'_>) -> Expr {
        let kind = match node.kind_name() {
            "group" => match field(node, "expr") {
                Some(choice) => return self.choice(&choice),
                None => ExprKind::Empty,
            },
            "LITERAL" => self.literal(node),
            "CLASS" => self.class(node),
            "PROPERTY" => self.property(node),
            "inherited" => ExprKind::Inherited,
            "any" => ExprKind::Any,
            "cut" => ExprKind::Cut,
            "reference" => ExprKind::Reference {
                name: self.field_name(node, "name"),
                args: field(node, "args")
                    .map(|a| fields(&a, "items").iter().map(|c| self.choice(c)).collect())
                    .unwrap_or_default(),
            },
            "flag_on" => ExprKind::FlagOn {
                flag: self.field_name(node, "flag"),
                expr: Box::new(self.field_choice(node, "expr")),
            },
            "flag_off" => ExprKind::FlagOff {
                flag: self.field_name(node, "flag"),
                expr: Box::new(self.field_choice(node, "expr")),
            },
            "flag_test" => ExprKind::FlagTest {
                flag: self.field_name(node, "flag"),
                negated: field(node, "negated").is_some(),
            },
            "indent" => ExprKind::Indent(Box::new(self.field_choice(node, "expr"))),
            "same" => ExprKind::Same,
            "def" => ExprKind::Def {
                name: self.field_name(node, "name"),
                expr: Box::new(self.field_choice(node, "expr")),
            },
            "ref" => ExprKind::Ref {
                name: self.field_name(node, "name"),
            },
            "error_alternative" => ExprKind::Error {
                message: match field(node, "message") {
                    Some(m) => self.string(&m),
                    None => Text {
                        value: String::new(),
                        span: self.span(node),
                    },
                },
            },
            other => {
                self.error(
                    DiagnosticCode::Internal,
                    format!("unexpected {other}"),
                    self.span(node),
                );
                ExprKind::Empty
            }
        };
        Expr {
            kind,
            span: self.span(node),
        }
    }

    fn string(&mut self, node: &NodeRef<'_>) -> Text {
        let raw = self.text_of(node);
        let inner = raw.get(1..raw.len().saturating_sub(1)).unwrap_or("");
        Text {
            value: self.unescape(inner, self.span(node), false),
            span: self.span(node),
        }
    }

    fn literal(&mut self, node: &NodeRef<'_>) -> ExprKind {
        let raw = self.text_of(node);
        let case_insensitive = raw.ends_with('i');
        let quoted = raw.strip_suffix('i').unwrap_or(raw);
        let inner = quoted.get(1..quoted.len().saturating_sub(1)).unwrap_or("");
        ExprKind::Literal {
            text: self.unescape(inner, self.span(node), false),
            case_insensitive,
        }
    }

    /// Decode the escapes of a literal, or of a class when `in_class`.
    fn unescape(&mut self, raw: &str, at: Span, in_class: bool) -> String {
        let mut out = String::new();
        let mut chars = raw.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            let decoded = match chars.next() {
                Some('\\') => Some('\\'),
                Some('"') => Some('"'),
                Some('\'') => Some('\''),
                Some('n') => Some('\n'),
                Some('r') => Some('\r'),
                Some('t') => Some('\t'),
                Some('0') => Some('\0'),
                Some(c @ ('[' | ']' | '-' | '^')) if in_class => Some(c),
                Some('x') => {
                    let hex: String = chars.by_ref().take(2).collect();
                    // from_str_radix would also take a sign.
                    let digits = hex.len() == 2 && hex.chars().all(|c| c.is_ascii_hexdigit());
                    u32::from_str_radix(&hex, 16)
                        .ok()
                        .filter(|_| digits)
                        .and_then(char::from_u32)
                }
                Some('u') => {
                    let rest = chars.as_str();
                    let digits = rest
                        .strip_prefix('{')
                        .and_then(|r| r.split_once('}'))
                        .map(|(hex, _)| hex)
                        .filter(|hex| (1..=6).contains(&hex.len()))
                        .filter(|hex| hex.chars().all(|c| c.is_ascii_hexdigit()));
                    match digits {
                        Some(hex) => {
                            let value = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
                            for _ in 0..hex.len() + 2 {
                                chars.next();
                            }
                            value
                        }
                        None => None,
                    }
                }
                _ => None,
            };
            match decoded {
                Some(c) => out.push(c),
                None => self.error(DiagnosticCode::Escape, "invalid escape".into(), at),
            }
        }
        out
    }

    fn class(&mut self, node: &NodeRef<'_>) -> ExprKind {
        let raw = self.text_of(node);
        let at = self.span(node);
        let inner = raw.get(1..raw.len().saturating_sub(1)).unwrap_or("");
        let (negated, inner) = match inner.strip_prefix('^') {
            Some(rest) => (true, rest),
            None => (false, inner),
        };
        // Split into units: one character, an escape, or a property, each decoded.
        let mut units: Vec<ClassUnit> = Vec::new();
        let mut rest = inner;
        while let Some(c) = rest.chars().next() {
            if let Some(after) = rest
                .strip_prefix("\\p{")
                .or_else(|| rest.strip_prefix("\\P{"))
                && let Some((name, tail)) = after.split_once('}')
            {
                let negated = rest.starts_with("\\P");
                match Property::from_name(name) {
                    Some(property) => units.push(ClassUnit::Property(property, negated)),
                    None => self.error(
                        DiagnosticCode::Property,
                        format!("unknown Unicode property {name}"),
                        at,
                    ),
                }
                rest = tail;
                continue;
            }
            if c == '\\' {
                let escape_len = escape_length(rest);
                let decoded = self.unescape(&rest[..escape_len], at, true);
                units.extend(decoded.chars().map(ClassUnit::Char));
                rest = &rest[escape_len..];
            } else {
                units.push(if c == '-' {
                    ClassUnit::Dash
                } else {
                    ClassUnit::Char(c)
                });
                rest = &rest[c.len_utf8()..];
            }
        }
        let mut items = Vec::new();
        let mut i = 0;
        while i < units.len() {
            let range = match (&units[i], units.get(i + 1), units.get(i + 2)) {
                (first, Some(ClassUnit::Dash), Some(last)) => first.char().zip(last.char()),
                _ => None,
            };
            if let Some((a, b)) = range {
                items.push(ClassItem::Range(a, b));
                i += 3;
                continue;
            }
            items.push(match &units[i] {
                ClassUnit::Property(property, negated) => ClassItem::Property {
                    property: *property,
                    negated: *negated,
                },
                ClassUnit::Char(c) => ClassItem::Range(*c, *c),
                ClassUnit::Dash => ClassItem::Range('-', '-'),
            });
            i += 1;
        }
        ExprKind::Class(Class { negated, items })
    }

    fn property(&mut self, node: &NodeRef<'_>) -> ExprKind {
        let raw = self.text_of(node);
        let negated = raw.starts_with("\\P");
        let name = raw.get(3..raw.len().saturating_sub(1)).unwrap_or("");
        match Property::from_name(name) {
            Some(property) => ExprKind::Class(Class {
                negated: false,
                items: vec![ClassItem::Property { property, negated }],
            }),
            None => {
                self.error(
                    DiagnosticCode::Property,
                    format!("unknown Unicode property {name}"),
                    self.span(node),
                );
                ExprKind::Empty
            }
        }
    }

    fn builder(&mut self, node: &NodeRef<'_>) -> ValueBuilder {
        let Some(child) = visible(node).into_iter().next() else {
            return ValueBuilder::Constant(Constant::None);
        };
        match child.kind_name() {
            "call" => ValueBuilder::Call {
                name: self.field_name(&child, "name"),
                args: fields(&child, "args")
                    .iter()
                    .map(|arg| BuilderArg {
                        key: field(arg, "key").map(|k| self.name(&k)),
                        value: match field(arg, "value") {
                            Some(v) if v.kind_name() == "STAR" => BuilderValue::All,
                            Some(v) if v.kind_name() == "constant" => {
                                BuilderValue::Constant(self.constant(&v))
                            }
                            Some(v) => match named_constant(self.text_of(&v)) {
                                Some(constant) => BuilderValue::Constant(constant),
                                None => BuilderValue::Field(self.name(&v)),
                            },
                            None => BuilderValue::All,
                        },
                    })
                    .collect(),
            },
            "constant" => ValueBuilder::Constant(self.constant(&child)),
            _ => match named_constant(self.text_of(&child)) {
                Some(constant) => ValueBuilder::Constant(constant),
                None => ValueBuilder::Name(self.name(&child)),
            },
        }
    }

    fn constant(&mut self, node: &NodeRef<'_>) -> Constant {
        match field(node, "value") {
            Some(v) if v.kind_name() == "STRING" => Constant::String(self.string(&v).value),
            Some(v) => Constant::Number(self.text_of(&v).to_owned()),
            None => Constant::None,
        }
    }
}

fn repeat(expr: Expr, min: u32, max: Option<u32>) -> ExprKind {
    ExprKind::Repeat {
        expr: Box::new(expr),
        min,
        max,
    }
}

fn named_constant(text: &str) -> Option<Constant> {
    match text {
        "None" => Some(Constant::None),
        "True" => Some(Constant::Bool(true)),
        "False" => Some(Constant::Bool(false)),
        _ => None,
    }
}

/// The length in bytes of the escape at the start of `rest`, which starts with a backslash.
fn escape_length(rest: &str) -> usize {
    let mut chars = rest.char_indices().skip(1);
    match chars.next() {
        // Only ASCII digits belong to it, so the length ends on a character boundary.
        Some((i, 'x')) => {
            let digits = rest[i + 1..].chars().take(2);
            i + 1 + digits.take_while(char::is_ascii_hexdigit).count()
        }
        Some((i, 'u')) => rest[i..]
            .find('}')
            .map_or(rest.len(), |close| i + close + 1),
        Some((i, c)) => i + c.len_utf8(),
        None => rest.len(),
    }
}

/// One element of a class body after escapes are decoded.
enum ClassUnit {
    /// A character, written or escaped, where an escaped dash is a character too.
    Char(char),
    Dash,
    Property(Property, bool),
}

impl ClassUnit {
    /// The code point the unit stands for. A bare dash stands for itself only outside ranges.
    fn char(&self) -> Option<char> {
        match self {
            ClassUnit::Char(c) => Some(*c),
            ClassUnit::Dash => Some('-'),
            ClassUnit::Property(..) => None,
        }
    }
}
