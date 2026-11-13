// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! The grammar model: a grammar as written, with source spans, before any name is resolved.
//!
//! The text front end and the programmatic [`builder`] both produce this model. Resolving it
//! checks the names and translates it into the core language that engines run.

pub mod builder;
#[cfg(feature = "reference")]
mod meta;
mod print;
pub(crate) mod resolve;
#[cfg(feature = "reference")]
mod text;

pub use resolve::GrammarError;

/// How deeply the expressions of a grammar may nest. Passes over an expression recurse, so a
/// deeper grammar is rejected before they run.
pub const MAX_EXPRESSION_DEPTH: usize = 100;

/// A byte range in the grammar's source text. Grammars built in code have empty spans.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    /// Offset of the first byte.
    pub start: u32,
    /// Offset after the last byte.
    pub end: u32,
}

/// A name as written, with where it was written.
#[derive(Clone, Debug, PartialEq)]
pub struct Name {
    /// The name.
    pub text: String,
    /// Where the name stands in the source.
    pub span: Span,
}

impl Name {
    /// A name without a source position, for grammars built in code.
    pub fn new(text: impl Into<String>) -> Name {
        Name {
            text: text.into(),
            span: Span::default(),
        }
    }
}

/// A whole grammar: its rules and directives.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grammar {
    /// The rules, in source order.
    pub rules: Vec<Rule>,
    /// The `%ignore` expressions. The skip expression is their ordered choice.
    pub ignore: Vec<Expr>,
    /// The `%word` expression, if any.
    pub word: Option<Expr>,
    /// The flags declared with `%flag`.
    pub flags: Vec<Name>,
    /// The tab width set with `%tabs`, if any.
    pub tabs: Option<Count>,
    /// The literals declared with `%keywords soft`.
    pub soft_keywords: Vec<Text>,
    /// The `%import` directives.
    pub imports: Vec<Import>,
    /// The `%extends` directive, if any.
    pub extends: Option<Text>,
    /// The `%recover` directives.
    pub recovers: Vec<Recover>,
}

/// A number written in the grammar, such as a repetition bound or a tab width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Count {
    /// The value.
    pub value: u32,
    /// Where it stands in the source.
    pub span: Span,
}

/// A string written in the grammar, after its escapes are decoded.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    /// The decoded text.
    pub value: String,
    /// Where the string stands in the source, quotes included.
    pub span: Span,
}

/// What a rule produces, decided by the case of its name and its attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleKind {
    /// A lowercase rule: skips ignored text before its tokens and produces a node.
    Syntactic,
    /// An uppercase rule: matched as one token, nothing skipped inside.
    Terminal,
    /// A lowercase rule marked `@lexical`: produces a node, skips nothing between its elements.
    Lexical,
}

/// One rule of a grammar.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    /// The name, with any leading underscores.
    pub name: Name,
    /// `?name`: inlined when it has exactly one visible child.
    pub transparent: bool,
    /// The parameter names of a template rule.
    pub params: Vec<Name>,
    /// The name used in error messages, if one is given.
    pub display: Option<Text>,
    /// The attributes written above the rule.
    pub attributes: Vec<Name>,
    /// The body.
    pub body: RuleBody,
    /// The whole rule in the source.
    pub span: Span,
}

impl Rule {
    /// The kind of the rule, from the case of its name and the `@lexical` attribute.
    pub fn kind(&self) -> RuleKind {
        let first = self.name.text.trim_start_matches('_').chars().next();
        if first.is_some_and(|c| c.is_ascii_uppercase()) {
            RuleKind::Terminal
        } else if self.has_attribute("lexical") {
            RuleKind::Lexical
        } else {
            RuleKind::Syntactic
        }
    }

    /// Whether the name starts with an underscore: an inlined rule or a hidden terminal.
    pub fn is_underscored(&self) -> bool {
        self.name.text.starts_with('_')
    }

    /// Whether the rule carries the attribute.
    pub fn has_attribute(&self, name: &str) -> bool {
        self.attributes.iter().any(|a| a.text == name)
    }
}

/// The body of a rule.
#[derive(Clone, Debug, PartialEq)]
pub enum RuleBody {
    /// Ordered alternatives.
    Alternatives(Vec<Alternative>),
    /// A precedence block: operator levels from loosest to tightest.
    Precedence(Vec<Level>),
}

/// One level of a precedence block.
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    /// The level's name, local to the block.
    pub name: Name,
    /// The level's own alternatives, tried before falling through to the next level.
    pub alternatives: Vec<Alternative>,
}

/// A top-level alternative of a rule or a level, with its alias and value builder.
#[derive(Clone, Debug, PartialEq)]
pub struct Alternative {
    /// What the alternative matches.
    pub expr: Expr,
    /// `-> name`: the kind of node the alternative produces.
    pub alias: Option<Name>,
    /// `=> builder`: how a transform turns the node into a value.
    pub builder: Option<Builder>,
}

/// An expression with its source span.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    /// What the expression is.
    pub kind: ExprKind,
    /// Where it stands in the source.
    pub span: Span,
}

impl Expr {
    /// An expression without a source position, for grammars built in code.
    pub fn new(kind: ExprKind) -> Expr {
        Expr {
            kind,
            span: Span::default(),
        }
    }

    /// The direct subexpressions, arguments of references included.
    pub fn children(&self) -> Vec<&Expr> {
        match &self.kind {
            ExprKind::Reference { args, .. } => args.iter().collect(),
            ExprKind::Sequence(items) | ExprKind::Choice(items) => items.iter().collect(),
            ExprKind::Separated {
                item, separator, ..
            } => vec![item, separator],
            ExprKind::Repeat { expr, .. }
            | ExprKind::And(expr)
            | ExprKind::Not(expr)
            | ExprKind::Field { expr, .. }
            | ExprKind::Label { expr, .. }
            | ExprKind::FlagOn { expr, .. }
            | ExprKind::FlagOff { expr, .. }
            | ExprKind::Indent(expr)
            | ExprKind::Def { expr, .. } => vec![expr],
            _ => Vec::new(),
        }
    }

    /// A copy with every direct subexpression replaced by `f` of it.
    pub fn map_children(&self, f: &mut dyn FnMut(&Expr) -> Expr) -> Expr {
        let mut each = |items: &[Expr]| items.iter().map(&mut *f).collect::<Vec<_>>();
        let kind = match &self.kind {
            ExprKind::Reference { name, args } => ExprKind::Reference {
                name: name.clone(),
                args: each(args),
            },
            ExprKind::Sequence(items) => ExprKind::Sequence(each(items)),
            ExprKind::Choice(items) => ExprKind::Choice(each(items)),
            ExprKind::Separated {
                item,
                separator,
                at_least_one,
            } => ExprKind::Separated {
                item: Box::new(f(item)),
                separator: Box::new(f(separator)),
                at_least_one: *at_least_one,
            },
            ExprKind::Repeat { expr, min, max } => ExprKind::Repeat {
                expr: Box::new(f(expr)),
                min: *min,
                max: *max,
            },
            ExprKind::And(expr) => ExprKind::And(Box::new(f(expr))),
            ExprKind::Not(expr) => ExprKind::Not(Box::new(f(expr))),
            ExprKind::Field { name, expr } => ExprKind::Field {
                name: name.clone(),
                expr: Box::new(f(expr)),
            },
            ExprKind::Label { expr, label } => ExprKind::Label {
                expr: Box::new(f(expr)),
                label: label.clone(),
            },
            ExprKind::FlagOn { flag, expr } => ExprKind::FlagOn {
                flag: flag.clone(),
                expr: Box::new(f(expr)),
            },
            ExprKind::FlagOff { flag, expr } => ExprKind::FlagOff {
                flag: flag.clone(),
                expr: Box::new(f(expr)),
            },
            ExprKind::Indent(expr) => ExprKind::Indent(Box::new(f(expr))),
            ExprKind::Def { name, expr } => ExprKind::Def {
                name: name.clone(),
                expr: Box::new(f(expr)),
            },
            other => other.clone(),
        };
        Expr {
            kind,
            span: self.span,
        }
    }

    /// Whether a field occurs in the expression, not counting the rules it refers to.
    pub fn contains_field(&self) -> bool {
        matches!(self.kind, ExprKind::Field { .. })
            || self.children().into_iter().any(Expr::contains_field)
    }

    /// How many expressions this one holds, itself included, and how deeply they nest, this one
    /// being at depth 1. It does not recurse, so it is safe on expressions of any depth.
    #[cfg_attr(
        not(feature = "reference"),
        expect(
            dead_code,
            reason = "the resolver and the text front end serve only the reference interpreter yet"
        )
    )]
    pub(crate) fn measure(&self) -> (usize, usize) {
        let mut pending = vec![(self, 1)];
        let (mut size, mut depth) = (0, 0);
        while let Some((expr, level)) = pending.pop() {
            size += 1;
            depth = depth.max(level);
            pending.extend(expr.children().into_iter().map(|child| (child, level + 1)));
        }
        (size, depth)
    }
}

/// The forms an expression can take.
#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    /// `()`: the empty string.
    Empty,
    /// A literal, ASCII case-insensitive when written with `i`.
    Literal {
        /// The decoded text, never empty.
        text: String,
        /// Whether ASCII letters match in either case.
        case_insensitive: bool,
    },
    /// A character class or a Unicode property, matching one code point.
    Class(Class),
    /// `.`: any one code point.
    Any,
    /// A rule name, a parameter name, or a level name inside a precedence block.
    Reference {
        /// The name.
        name: Name,
        /// The arguments of a template rule.
        args: Vec<Expr>,
    },
    /// Expressions matched in turn.
    Sequence(Vec<Expr>),
    /// Ordered choice.
    Choice(Vec<Expr>),
    /// Repetition between `min` and `max` times. `?`, `*`, `+` and `{n,m}` all use it.
    Repeat {
        /// The repeated expression.
        expr: Box<Expr>,
        /// The minimum count.
        min: u32,
        /// The maximum count, unbounded if absent.
        max: Option<u32>,
    },
    /// `e ** s` or `e ++ s`.
    Separated {
        /// The repeated item.
        item: Box<Expr>,
        /// The separator.
        separator: Box<Expr>,
        /// `++`: at least one item.
        at_least_one: bool,
    },
    /// `&e`.
    And(Box<Expr>),
    /// `!e`.
    Not(Box<Expr>),
    /// `label=e`.
    Field {
        /// The field name.
        name: Name,
        /// The labeled expression.
        expr: Box<Expr>,
    },
    /// `~`.
    Cut,
    /// `e ^ "message"` or `e ^ label`.
    Label {
        /// The expression whose failure is an error.
        expr: Box<Expr>,
        /// The error.
        label: ErrorLabel,
    },
    /// `%on(flag, e)`.
    FlagOn {
        /// The flag.
        flag: Name,
        /// The expression matched with the flag set.
        expr: Box<Expr>,
    },
    /// `%off(flag, e)`.
    FlagOff {
        /// The flag.
        flag: Name,
        /// The expression matched with the flag cleared.
        expr: Box<Expr>,
    },
    /// `%if(flag)` or `%if(!flag)`.
    FlagTest {
        /// The flag.
        flag: Name,
        /// `%if(!flag)`.
        negated: bool,
    },
    /// `%indent(e)`.
    Indent(Box<Expr>),
    /// `%same`.
    Same,
    /// `%def(name, e)`.
    Def {
        /// The capture name.
        name: Name,
        /// The expression whose text is remembered.
        expr: Box<Expr>,
    },
    /// `%ref(name)`.
    Ref {
        /// The capture name.
        name: Name,
    },
    /// `%error("message")`.
    Error {
        /// The message.
        message: Text,
    },
    /// `...`: the inherited body in a grammar that extends another.
    Inherited,
}

/// The error of an error label.
#[derive(Clone, Debug, PartialEq)]
pub enum ErrorLabel {
    /// `^ "message"`.
    Message(Text),
    /// `^ label`, recovered by a `%recover` directive.
    Named(Name),
}

/// A character class: listed code points and ranges, or a Unicode property.
#[derive(Clone, Debug, PartialEq)]
pub struct Class {
    /// `[^...]` or `\P{...}`: matches what is not listed.
    pub negated: bool,
    /// The listed ranges and properties.
    pub items: Vec<ClassItem>,
}

/// One entry of a character class.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClassItem {
    /// A range of code points, both ends included. A single code point is a range of one.
    Range(char, char),
    /// A Unicode property, or its complement.
    Property {
        /// The property.
        property: Property,
        /// `\P{...}`: code points without the property.
        negated: bool,
    },
}

/// The Unicode properties a class may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Property {
    /// `XID_Start`.
    XidStart,
    /// `XID_Continue`.
    XidContinue,
    /// `Alphabetic`.
    Alphabetic,
    /// `White_Space`.
    WhiteSpace,
    /// `Uppercase`.
    Uppercase,
    /// `Lowercase`.
    Lowercase,
}

impl Property {
    /// All properties with the names grammars use for them.
    pub const ALL: [(Property, &'static str); 6] = [
        (Property::XidStart, "XID_Start"),
        (Property::XidContinue, "XID_Continue"),
        (Property::Alphabetic, "Alphabetic"),
        (Property::WhiteSpace, "White_Space"),
        (Property::Uppercase, "Uppercase"),
        (Property::Lowercase, "Lowercase"),
    ];

    /// The property with this name, if it is supported.
    pub fn from_name(name: &str) -> Option<Property> {
        Property::ALL
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(p, _)| *p)
    }

    /// The name grammars use for the property.
    pub fn name(self) -> &'static str {
        Property::ALL
            .iter()
            .find(|(p, _)| *p == self)
            .map_or("", |(_, n)| n)
    }

    /// Whether the code point has the property.
    pub fn contains(self, c: char) -> bool {
        match self {
            Property::XidStart => unicode_ident::is_xid_start(c),
            Property::XidContinue => unicode_ident::is_xid_continue(c),
            Property::Alphabetic => c.is_alphabetic(),
            Property::WhiteSpace => c.is_whitespace(),
            Property::Uppercase => c.is_uppercase(),
            Property::Lowercase => c.is_lowercase(),
        }
    }
}

/// A value builder written after `=>`.
#[derive(Clone, Debug, PartialEq)]
pub enum Builder {
    /// A named builder such as `int`, `list` or `dict`, or a registered function used bare.
    Name(Name),
    /// A constant value.
    Constant(Constant),
    /// A call of a registered function.
    Call {
        /// The function.
        name: Name,
        /// The arguments.
        args: Vec<BuilderArg>,
    },
}

/// A constant in a value builder.
#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    /// `None`.
    None,
    /// `True` or `False`.
    Bool(bool),
    /// A number, kept as written so that integers and decimals stay apart.
    Number(String),
    /// A string.
    String(String),
}

/// One argument of a builder call.
#[derive(Clone, Debug, PartialEq)]
pub struct BuilderArg {
    /// The keyword, for `key=value`.
    pub key: Option<Name>,
    /// The value.
    pub value: BuilderValue,
}

/// The value of a builder argument.
#[derive(Clone, Debug, PartialEq)]
pub enum BuilderValue {
    /// A field of the node.
    Field(Name),
    /// A constant.
    Constant(Constant),
    /// `*`: all visible children.
    All,
}

/// An `%import` directive.
#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    /// The grammar imported from.
    pub module: Text,
    /// The rules taken, all of them if empty.
    pub names: Vec<Name>,
    /// `as prefix`.
    pub prefix: Option<Name>,
}

/// A `%recover label: e` directive.
#[derive(Clone, Debug, PartialEq)]
pub struct Recover {
    /// The error label.
    pub label: Name,
    /// The recovery expression.
    pub expr: Expr,
}
