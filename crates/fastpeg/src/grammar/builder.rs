// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Build grammars in code.
//!
//! ```
//! use fastpeg::builder::*;
//!
//! let mut g = GrammarBuilder::new();
//! g.rule("start", seq([r("NUMBER"), many(seq([lit("+"), r("NUMBER")]))]));
//! g.rule("NUMBER", some(class([range('0', '9')])));
//! g.ignore(some(class([ch(' ')])));
//! let grammar = g.build();
//! assert_eq!(grammar.rules.len(), 2);
//! ```

use super::{
    Alternative, Class, ClassItem, ErrorLabel, Expr, ExprKind, Grammar, Level, Name, Property,
    Rule, RuleBody, Span, Text,
};

/// Collects rules and directives into a [`Grammar`].
#[derive(Debug, Default)]
pub struct GrammarBuilder {
    grammar: Grammar,
}

impl GrammarBuilder {
    /// An empty grammar.
    pub fn new() -> GrammarBuilder {
        GrammarBuilder::default()
    }

    /// Add a rule and return it for further settings. A top-level choice becomes the rule's
    /// alternatives, as `|` at the top of a rule does in grammar text.
    pub fn rule(&mut self, name: &str, body: Expr) -> &mut Rule {
        let alternatives = match body.kind {
            ExprKind::Choice(items) => items.into_iter().map(alt).collect(),
            _ => vec![alt(body)],
        };
        self.rule_alternatives(name, alternatives)
    }

    /// Add a rule with several alternatives, each of which may carry an alias.
    pub fn rule_alternatives(&mut self, name: &str, alternatives: Vec<Alternative>) -> &mut Rule {
        self.push(name, RuleBody::Alternatives(alternatives))
    }

    /// Add a rule made of a precedence block.
    pub fn precedence(&mut self, name: &str, levels: Vec<Level>) -> &mut Rule {
        self.push(name, RuleBody::Precedence(levels))
    }

    fn push(&mut self, name: &str, body: RuleBody) -> &mut Rule {
        self.grammar.rules.push(Rule {
            name: Name::new(name),
            transparent: false,
            params: Vec::new(),
            display: None,
            attributes: Vec::new(),
            body,
            span: Span::default(),
        });
        let last = self.grammar.rules.len() - 1;
        &mut self.grammar.rules[last]
    }

    /// Add an `%ignore` expression.
    pub fn ignore(&mut self, expr: Expr) -> &mut GrammarBuilder {
        self.grammar.ignore.push(expr);
        self
    }

    /// Declare a flag.
    pub fn flag(&mut self, name: &str) -> &mut GrammarBuilder {
        self.grammar.flags.push(Name::new(name));
        self
    }

    /// Set the word characters.
    pub fn word(&mut self, expr: Expr) -> &mut GrammarBuilder {
        self.grammar.word = Some(expr);
        self
    }

    /// The finished grammar.
    pub fn build(self) -> Grammar {
        self.grammar
    }
}

impl Rule {
    /// Make the rule transparent: inlined when it has exactly one visible child.
    pub fn transparent(&mut self) -> &mut Rule {
        self.transparent = true;
        self
    }

    /// Add an attribute such as `lexical` or `reserved`.
    pub fn attribute(&mut self, name: &str) -> &mut Rule {
        self.attributes.push(Name::new(name));
        self
    }

    /// Set the name error messages use.
    pub fn display(&mut self, text: &str) -> &mut Rule {
        self.display = Some(text_value(text));
        self
    }

    /// Declare the parameters of a template rule.
    pub fn params<const N: usize>(&mut self, names: [&str; N]) -> &mut Rule {
        self.params = names.iter().map(|n| Name::new(*n)).collect();
        self
    }
}

impl Alternative {
    /// Name the node this alternative produces.
    pub fn alias(mut self, name: &str) -> Alternative {
        self.alias = Some(Name::new(name));
        self
    }
}

/// An alternative without alias or builder.
pub fn alt(expr: Expr) -> Alternative {
    Alternative {
        expr,
        alias: None,
        builder: None,
    }
}

/// A level of a precedence block.
pub fn level(name: &str, alternatives: Vec<Alternative>) -> Level {
    Level {
        name: Name::new(name),
        alternatives,
    }
}

fn text_value(text: &str) -> Text {
    Text {
        value: text.to_owned(),
        span: Span::default(),
    }
}

fn boxed(kind: ExprKind) -> Expr {
    Expr::new(kind)
}

/// `()`: the empty string.
pub fn empty() -> Expr {
    boxed(ExprKind::Empty)
}

/// A literal.
pub fn lit(text: &str) -> Expr {
    boxed(ExprKind::Literal {
        text: text.to_owned(),
        case_insensitive: false,
    })
}

/// An ASCII case-insensitive literal, `"text"i`.
pub fn lit_i(text: &str) -> Expr {
    boxed(ExprKind::Literal {
        text: text.to_owned(),
        case_insensitive: true,
    })
}

/// A character class, `[...]`.
pub fn class<const N: usize>(items: [ClassItem; N]) -> Expr {
    boxed(ExprKind::Class(Class {
        negated: false,
        items: items.to_vec(),
    }))
}

/// A negated character class, `[^...]`.
pub fn not_class<const N: usize>(items: [ClassItem; N]) -> Expr {
    boxed(ExprKind::Class(Class {
        negated: true,
        items: items.to_vec(),
    }))
}

/// A range of code points for a class.
pub fn range(first: char, last: char) -> ClassItem {
    ClassItem::Range(first, last)
}

/// A single code point for a class.
pub fn ch(c: char) -> ClassItem {
    ClassItem::Range(c, c)
}

/// A Unicode property for a class.
pub fn prop(property: Property) -> ClassItem {
    ClassItem::Property {
        property,
        negated: false,
    }
}

/// `.`: any one code point.
pub fn any() -> Expr {
    boxed(ExprKind::Any)
}

/// A reference to a rule, parameter or level.
pub fn r(name: &str) -> Expr {
    boxed(ExprKind::Reference {
        name: Name::new(name),
        args: Vec::new(),
    })
}

/// A use of a template rule with arguments.
pub fn r_args<const N: usize>(name: &str, args: [Expr; N]) -> Expr {
    boxed(ExprKind::Reference {
        name: Name::new(name),
        args: args.into(),
    })
}

/// A sequence.
pub fn seq<const N: usize>(items: [Expr; N]) -> Expr {
    boxed(ExprKind::Sequence(items.into()))
}

/// An ordered choice.
pub fn choice<const N: usize>(items: [Expr; N]) -> Expr {
    boxed(ExprKind::Choice(items.into()))
}

/// Repetition between `min` and `max` times, unbounded if `max` is `None`.
pub fn rep(expr: Expr, min: u32, max: Option<u32>) -> Expr {
    boxed(ExprKind::Repeat {
        expr: Box::new(expr),
        min,
        max,
    })
}

/// `e?`.
pub fn opt(expr: Expr) -> Expr {
    rep(expr, 0, Some(1))
}

/// `e*`.
pub fn many(expr: Expr) -> Expr {
    rep(expr, 0, None)
}

/// `e+`.
pub fn some(expr: Expr) -> Expr {
    rep(expr, 1, None)
}

/// `e ** s`: zero or more items separated by `s`.
pub fn sep_many(item: Expr, separator: Expr) -> Expr {
    separated(item, separator, false)
}

/// `e ++ s`: one or more items separated by `s`.
pub fn sep_some(item: Expr, separator: Expr) -> Expr {
    separated(item, separator, true)
}

fn separated(item: Expr, separator: Expr, at_least_one: bool) -> Expr {
    boxed(ExprKind::Separated {
        item: Box::new(item),
        separator: Box::new(separator),
        at_least_one,
    })
}

/// `&e`.
pub fn and(expr: Expr) -> Expr {
    boxed(ExprKind::And(Box::new(expr)))
}

/// `!e`.
pub fn not(expr: Expr) -> Expr {
    boxed(ExprKind::Not(Box::new(expr)))
}

/// `label=e`.
pub fn field(name: &str, expr: Expr) -> Expr {
    boxed(ExprKind::Field {
        name: Name::new(name),
        expr: Box::new(expr),
    })
}

/// `~`.
pub fn cut() -> Expr {
    boxed(ExprKind::Cut)
}

/// `e ^ "message"`.
pub fn label_message(expr: Expr, message: &str) -> Expr {
    boxed(ExprKind::Label {
        expr: Box::new(expr),
        label: ErrorLabel::Message(text_value(message)),
    })
}

/// `%on(flag, e)`.
pub fn on(flag: &str, expr: Expr) -> Expr {
    boxed(ExprKind::FlagOn {
        flag: Name::new(flag),
        expr: Box::new(expr),
    })
}

/// `%off(flag, e)`.
pub fn off(flag: &str, expr: Expr) -> Expr {
    boxed(ExprKind::FlagOff {
        flag: Name::new(flag),
        expr: Box::new(expr),
    })
}

/// `%if(flag)`.
pub fn if_flag(flag: &str) -> Expr {
    boxed(ExprKind::FlagTest {
        flag: Name::new(flag),
        negated: false,
    })
}

/// `%if(!flag)`.
pub fn if_not_flag(flag: &str) -> Expr {
    boxed(ExprKind::FlagTest {
        flag: Name::new(flag),
        negated: true,
    })
}

/// `%indent(e)`.
pub fn indent(expr: Expr) -> Expr {
    boxed(ExprKind::Indent(Box::new(expr)))
}

/// `%same`.
pub fn same() -> Expr {
    boxed(ExprKind::Same)
}

/// `%def(name, e)`.
pub fn def(name: &str, expr: Expr) -> Expr {
    boxed(ExprKind::Def {
        name: Name::new(name),
        expr: Box::new(expr),
    })
}

/// `%ref(name)`.
pub fn capture_ref(name: &str) -> Expr {
    boxed(ExprKind::Ref {
        name: Name::new(name),
    })
}
