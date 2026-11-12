// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! The canonical text of a grammar. Parsing the text gives back an equal grammar, and printing
//! that grammar gives the same text, which makes the printer an oracle for the front end.

use std::fmt::{self, Display, Formatter, Write};

use super::{
    Alternative, Builder, BuilderArg, BuilderValue, Class, ClassItem, Constant, ErrorLabel, Expr,
    ExprKind, Grammar, Rule, RuleBody,
};

/// How tightly an expression form binds, loosest first. A part that binds looser than its
/// position allows is put in parentheses.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Strength {
    Choice,
    Sequence,
    Label,
    Prefix,
    Suffix,
    Primary,
}

fn strength(expr: &Expr) -> Strength {
    match &expr.kind {
        ExprKind::Choice(_) => Strength::Choice,
        ExprKind::Sequence(items) if items.len() > 1 => Strength::Sequence,
        ExprKind::Label { .. } => Strength::Label,
        ExprKind::And(_) | ExprKind::Not(_) | ExprKind::Field { .. } => Strength::Prefix,
        ExprKind::Repeat { .. } | ExprKind::Separated { .. } => Strength::Suffix,
        _ => Strength::Primary,
    }
}

impl Display for Grammar {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(module) = &self.extends {
            writeln!(f, "%extends {}", quoted(&module.value))?;
        }
        for import in &self.imports {
            write!(f, "%import {}", module_name(&import.module.value))?;
            if !import.names.is_empty() {
                let names: Vec<&str> = import.names.iter().map(|n| n.text.as_str()).collect();
                write!(f, " ({})", names.join(", "))?;
            }
            if let Some(prefix) = &import.prefix {
                write!(f, " as {}", prefix.text)?;
            }
            writeln!(f)?;
        }
        if !self.flags.is_empty() {
            let names: Vec<&str> = self.flags.iter().map(|n| n.text.as_str()).collect();
            writeln!(f, "%flag {}", names.join(" "))?;
        }
        if let Some(word) = &self.word {
            writeln!(f, "%word {}", ExprText(word, Strength::Choice))?;
        }
        if let Some(tabs) = &self.tabs {
            writeln!(f, "%tabs {}", tabs.value)?;
        }
        if !self.soft_keywords.is_empty() {
            let words: Vec<String> = self
                .soft_keywords
                .iter()
                .map(|t| quoted(&t.value))
                .collect();
            writeln!(f, "%keywords soft {}", words.join(" "))?;
        }
        for ignore in &self.ignore {
            writeln!(f, "%ignore {}", ExprText(ignore, Strength::Choice))?;
        }
        for rule in &self.rules {
            write_rule(f, rule)?;
        }
        for recover in &self.recovers {
            let expr = ExprText(&recover.expr, Strength::Choice);
            writeln!(f, "%recover {}: {expr}", recover.label.text)?;
        }
        Ok(())
    }
}

fn write_rule(f: &mut Formatter<'_>, rule: &Rule) -> fmt::Result {
    for attribute in &rule.attributes {
        writeln!(f, "@{}", attribute.text)?;
    }
    if rule.transparent {
        f.write_char('?')?;
    }
    f.write_str(&rule.name.text)?;
    if !rule.params.is_empty() {
        let names: Vec<&str> = rule.params.iter().map(|n| n.text.as_str()).collect();
        write!(f, "<{}>", names.join(", "))?;
    }
    if let Some(display) = &rule.display {
        write!(f, " {}", quoted(&display.value))?;
    }
    f.write_str(": ")?;
    match &rule.body {
        RuleBody::Alternatives(alternatives) => write_alternatives(f, alternatives, "    ")?,
        RuleBody::Precedence(levels) => {
            f.write_str("%precedence {\n")?;
            for level in levels {
                write!(f, "    {}: ", level.name.text)?;
                write_alternatives(f, &level.alternatives, "        ")?;
            }
            f.write_str("}\n")?;
        }
    }
    Ok(())
}

fn write_alternatives(
    f: &mut Formatter<'_>,
    alternatives: &[Alternative],
    pad: &str,
) -> fmt::Result {
    for (i, alternative) in alternatives.iter().enumerate() {
        if i > 0 {
            write!(f, "\n{pad}| ")?;
        }
        write!(f, "{}", ExprText(&alternative.expr, Strength::Sequence))?;
        if let Some(alias) = &alternative.alias {
            write!(f, " -> {}", alias.text)?;
        }
        if let Some(builder) = &alternative.builder {
            write!(f, " => {}", BuilderText(builder))?;
        }
    }
    f.write_char('\n')
}

/// The canonical text of an expression, as it would appear in a grammar.
#[cfg_attr(
    not(feature = "reference"),
    expect(
        dead_code,
        reason = "the resolver uses it, which serves only the reference interpreter yet"
    )
)]
pub(crate) fn expr_text(expr: &Expr) -> String {
    ExprText(expr, Strength::Choice).to_string()
}

/// An expression printed at a position that allows the given strength.
struct ExprText<'a>(&'a Expr, Strength);

impl Display for ExprText<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let ExprText(mut expr, allowed) = *self;
        // A sequence or choice of one item means the item, and prints as the item does.
        while let ExprKind::Sequence(items) | ExprKind::Choice(items) = &expr.kind {
            match items.as_slice() {
                [only] => expr = only,
                _ => break,
            }
        }
        if strength(expr) < allowed {
            write!(f, "({})", ExprText(expr, Strength::Choice))
        } else {
            write_expr(f, expr)
        }
    }
}

fn write_expr(f: &mut Formatter<'_>, expr: &Expr) -> fmt::Result {
    match &expr.kind {
        ExprKind::Empty => f.write_str("()"),
        ExprKind::Literal {
            text,
            case_insensitive,
        } => {
            f.write_str(&quoted(text))?;
            if *case_insensitive {
                f.write_char('i')?;
            }
            Ok(())
        }
        ExprKind::Class(class) => write_class(f, class),
        ExprKind::Any => f.write_char('.'),
        ExprKind::Reference { name, args } => {
            f.write_str(&name.text)?;
            if !args.is_empty() {
                f.write_char('<')?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}", ExprText(arg, Strength::Choice))?;
                }
                f.write_char('>')?;
            }
            Ok(())
        }
        ExprKind::Sequence(items) if items.is_empty() => f.write_str("()"),
        ExprKind::Sequence(items) => join(f, items, " ", Strength::Label),
        ExprKind::Choice(items) => join(f, items, " | ", Strength::Sequence),
        ExprKind::Repeat { expr, min, max } => {
            write!(f, "{}", ExprText(expr, Strength::Suffix))?;
            match (min, max) {
                (0, Some(1)) => f.write_char('?'),
                (0, None) => f.write_char('*'),
                (1, None) => f.write_char('+'),
                (n, None) => write!(f, "{{{n},}}"),
                (n, Some(m)) if n == m => write!(f, "{{{n}}}"),
                (n, Some(m)) => write!(f, "{{{n},{m}}}"),
            }
        }
        ExprKind::Separated {
            item,
            separator,
            at_least_one,
        } => {
            let op = if *at_least_one { "++" } else { "**" };
            let item = ExprText(item, Strength::Suffix);
            write!(f, "{item} {op} {}", ExprText(separator, Strength::Primary))
        }
        ExprKind::And(inner) => write!(f, "&{}", ExprText(inner, Strength::Prefix)),
        ExprKind::Not(inner) => write!(f, "!{}", ExprText(inner, Strength::Prefix)),
        ExprKind::Field { name, expr } => {
            write!(f, "{}={}", name.text, ExprText(expr, Strength::Suffix))
        }
        ExprKind::Cut => f.write_char('~'),
        ExprKind::Label { expr, label } => {
            write!(f, "{} ^ ", ExprText(expr, Strength::Prefix))?;
            match label {
                ErrorLabel::Message(text) => f.write_str(&quoted(&text.value)),
                ErrorLabel::Named(name) => f.write_str(&name.text),
            }
        }
        ExprKind::FlagOn { flag, expr } => {
            write!(
                f,
                "%on({}, {})",
                flag.text,
                ExprText(expr, Strength::Choice)
            )
        }
        ExprKind::FlagOff { flag, expr } => {
            write!(
                f,
                "%off({}, {})",
                flag.text,
                ExprText(expr, Strength::Choice)
            )
        }
        ExprKind::FlagTest { flag, negated } => {
            let bang = if *negated { "!" } else { "" };
            write!(f, "%if({bang}{})", flag.text)
        }
        ExprKind::Indent(inner) => write!(f, "%indent({})", ExprText(inner, Strength::Choice)),
        ExprKind::Same => f.write_str("%same"),
        ExprKind::Def { name, expr } => {
            write!(
                f,
                "%def({}, {})",
                name.text,
                ExprText(expr, Strength::Choice)
            )
        }
        ExprKind::Ref { name } => write!(f, "%ref({})", name.text),
        ExprKind::Error { message } => write!(f, "%error({})", quoted(&message.value)),
        ExprKind::Inherited => f.write_str("..."),
    }
}

fn join(f: &mut Formatter<'_>, items: &[Expr], sep: &str, allowed: Strength) -> fmt::Result {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            f.write_str(sep)?;
        }
        write!(f, "{}", ExprText(item, allowed))?;
    }
    Ok(())
}

fn write_class(f: &mut Formatter<'_>, class: &Class) -> fmt::Result {
    if let (false, [ClassItem::Property { property, negated }]) =
        (class.negated, class.items.as_slice())
    {
        let p = if *negated { 'P' } else { 'p' };
        return write!(f, "\\{p}{{{}}}", property.name());
    }
    f.write_char('[')?;
    if class.negated {
        f.write_char('^')?;
    }
    for item in &class.items {
        match *item {
            ClassItem::Range(a, b) if a == b => f.write_str(&class_char(a))?,
            ClassItem::Range(a, b) => write!(f, "{}-{}", class_char(a), class_char(b))?,
            ClassItem::Property { property, negated } => {
                let p = if negated { 'P' } else { 'p' };
                write!(f, "\\{p}{{{}}}", property.name())?;
            }
        }
    }
    f.write_char(']')
}

/// A string in double quotes with the escapes the grammar language defines.
pub(crate) fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            _ => push_escaped(&mut out, c),
        }
    }
    out.push('"');
    out
}

fn class_char(c: char) -> String {
    let mut out = String::new();
    match c {
        ']' | '[' | '^' | '-' => {
            out.push('\\');
            out.push(c);
        }
        _ => push_escaped(&mut out, c),
    }
    out
}

fn push_escaped(out: &mut String, c: char) {
    match c {
        '\\' => out.push_str("\\\\"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        '\0' => out.push_str("\\0"),
        c if c.is_control() => {
            let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
        }
        c => out.push(c),
    }
}

fn module_name(module: &str) -> String {
    let plain = module
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && module
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if plain {
        module.to_owned()
    } else {
        quoted(module)
    }
}

struct BuilderText<'a>(&'a Builder);

impl Display for BuilderText<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.0 {
            Builder::Name(name) => f.write_str(&name.text),
            Builder::Constant(constant) => f.write_str(&constant_text(constant)),
            Builder::Call { name, args } => {
                write!(f, "{}(", name.text)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write_builder_arg(f, arg)?;
                }
                f.write_char(')')
            }
        }
    }
}

fn write_builder_arg(f: &mut Formatter<'_>, arg: &BuilderArg) -> fmt::Result {
    if let Some(key) = &arg.key {
        write!(f, "{}=", key.text)?;
    }
    match &arg.value {
        BuilderValue::Field(name) => f.write_str(&name.text),
        BuilderValue::Constant(constant) => f.write_str(&constant_text(constant)),
        BuilderValue::All => f.write_char('*'),
    }
}

fn constant_text(constant: &Constant) -> String {
    match constant {
        Constant::None => "None".to_owned(),
        Constant::Bool(true) => "True".to_owned(),
        Constant::Bool(false) => "False".to_owned(),
        Constant::Number(text) => text.clone(),
        Constant::String(text) => quoted(text),
    }
}
