// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bootstrap of the front end, compiled only for tests. `meta.rs`, the grammar of grammars
//! as builder code, is printed here from `crates/fpeg/grammars/fpeg.fpeg`, so that the text is
//! the source and the code its copy. A test holds the file to what is printed, and
//! `FPEG_BLESS=1 cargo test` rewrites it, to review as a diff before a commit.
//!
//! The printer knows what the builder can say. A grammar of grammars that needs more fails
//! the test with the name of the form, and the builder and the printer learn it then.

use super::{Alternative, Class, ClassItem, ErrorLabel, Expr, ExprKind, Grammar, Rule, RuleBody};

/// The longest line of the printed code, which is rustfmt's.
const MAX_WIDTH_CHARS: usize = 100;

/// One step of indentation.
const INDENT: &str = "    ";

/// Builder code: text that stays on one line, or a call whose arguments go on lines of their
/// own where the call does not fit on one.
enum Code {
    Text(String),
    Call {
        open: String,
        args: Vec<Code>,
        close: String,
    },
}

impl Code {
    fn call(name: &str, args: Vec<Code>) -> Code {
        Code::Call {
            open: format!("{name}("),
            args,
            close: ")".to_owned(),
        }
    }

    /// A call that takes its arguments as one array.
    fn array(name: &str, items: Vec<Code>) -> Code {
        Code::Call {
            open: format!("{name}(["),
            args: items,
            close: "])".to_owned(),
        }
    }

    fn vec(items: Vec<Code>) -> Code {
        Code::Call {
            open: "vec![".to_owned(),
            args: items,
            close: "]".to_owned(),
        }
    }

    /// A string literal. The debug form of a string is one.
    fn string(text: &str) -> Code {
        Code::Text(format!("{text:?}"))
    }

    /// The code with `more` after its closing bracket, such as a method call.
    fn then(self, more: &str) -> Code {
        match self {
            Code::Text(text) => Code::Text(format!("{text}{more}")),
            Code::Call { open, args, close } => Code::Call {
                open,
                args,
                close: format!("{close}{more}"),
            },
        }
    }

    fn flat(&self) -> String {
        match self {
            Code::Text(text) => text.clone(),
            Code::Call { open, args, close } => {
                let args: Vec<String> = args.iter().map(Code::flat).collect();
                format!("{open}{}{close}", args.join(", "))
            }
        }
    }

    /// Write the code, which starts at `column` of a line indented by `depth` steps and has
    /// `after` behind it on its last line.
    fn write(&self, depth: usize, column: usize, after: &str, out: &mut String) {
        let flat = self.flat();
        let fits = column + flat.chars().count() + after.len() <= MAX_WIDTH_CHARS;
        let Code::Call { open, args, close } = self else {
            out.push_str(&flat);
            return;
        };
        if fits {
            out.push_str(&flat);
            return;
        }
        out.push_str(open);
        if let [only @ Code::Call { .. }] = args.as_slice() {
            // A lone argument that is a call stays on the line of the call around it.
            only.write(depth, column + open.len(), &format!("{close}{after}"), out);
        } else {
            for arg in args {
                out.push('\n');
                out.push_str(&INDENT.repeat(depth + 1));
                arg.write(depth + 1, (depth + 1) * INDENT.len(), ",", out);
                out.push(',');
            }
            out.push('\n');
            out.push_str(&INDENT.repeat(depth));
        }
        out.push_str(close);
    }
}

/// What the builder has no word for. The test that prints the bootstrap fails with it.
fn unknown(what: &str) -> ! {
    panic!("the printer of the bootstrap does not know {what}: teach the builder and the printer")
}

fn class(class: &Class) -> Code {
    let items = class.items.iter().map(|item| match item {
        ClassItem::Range(first, last) if first == last => Code::Text(format!("ch({first:?})")),
        ClassItem::Range(first, last) => Code::Text(format!("range({first:?}, {last:?})")),
        ClassItem::Property { .. } => unknown("a property in a class"),
    });
    let name = if class.negated { "not_class" } else { "class" };
    Code::array(name, items.collect())
}

fn expr(expr: &Expr) -> Code {
    let each = |items: &[Expr]| items.iter().map(self::expr).collect::<Vec<_>>();
    match &expr.kind {
        ExprKind::Empty => Code::call("empty", vec![]),
        ExprKind::Literal {
            text,
            case_insensitive,
        } => {
            let name = if *case_insensitive { "lit_i" } else { "lit" };
            Code::call(name, vec![Code::string(text)])
        }
        ExprKind::Class(items) => class(items),
        ExprKind::Any => Code::call("any", vec![]),
        ExprKind::Reference { name, args } if args.is_empty() => {
            Code::call("r", vec![Code::string(&name.text)])
        }
        ExprKind::Reference { name, args } => Code::call(
            "r_args",
            vec![
                Code::string(&name.text),
                Code::Call {
                    open: "[".to_owned(),
                    args: each(args),
                    close: "]".to_owned(),
                },
            ],
        ),
        ExprKind::Sequence(items) => Code::array("seq", each(items)),
        ExprKind::Choice(items) => Code::array("choice", each(items)),
        ExprKind::Repeat { expr, min, max } => {
            let inner = self::expr(expr);
            match (min, max) {
                (0, Some(1)) => Code::call("opt", vec![inner]),
                (0, None) => Code::call("many", vec![inner]),
                (1, None) => Code::call("some", vec![inner]),
                _ => {
                    let bounds = [Code::Text(min.to_string()), Code::Text(format!("{max:?}"))];
                    Code::call("rep", std::iter::once(inner).chain(bounds).collect())
                }
            }
        }
        ExprKind::Separated {
            item,
            separator,
            at_least_one,
        } => {
            let name = if *at_least_one {
                "sep_some"
            } else {
                "sep_many"
            };
            Code::call(name, vec![self::expr(item), self::expr(separator)])
        }
        ExprKind::And(inner) => Code::call("and", vec![self::expr(inner)]),
        ExprKind::Not(inner) => Code::call("not", vec![self::expr(inner)]),
        ExprKind::Field { name, expr } => {
            Code::call("field", vec![Code::string(&name.text), self::expr(expr)])
        }
        ExprKind::Cut => Code::call("cut", vec![]),
        ExprKind::Label {
            expr,
            label: ErrorLabel::Message(message),
        } => Code::call(
            "label_message",
            vec![self::expr(expr), Code::string(&message.value)],
        ),
        ExprKind::Label { .. } => unknown("an error label by name"),
        ExprKind::FlagOn { flag, expr } => {
            Code::call("on", vec![Code::string(&flag.text), self::expr(expr)])
        }
        ExprKind::FlagOff { flag, expr } => {
            Code::call("off", vec![Code::string(&flag.text), self::expr(expr)])
        }
        ExprKind::FlagTest { flag, negated } => {
            let name = if *negated { "if_not_flag" } else { "if_flag" };
            Code::call(name, vec![Code::string(&flag.text)])
        }
        ExprKind::Indent(inner) => Code::call("indent", vec![self::expr(inner)]),
        ExprKind::Same => Code::call("same", vec![]),
        ExprKind::Def { name, expr } => {
            Code::call("def", vec![Code::string(&name.text), self::expr(expr)])
        }
        ExprKind::Ref { name } => Code::call("capture_ref", vec![Code::string(&name.text)]),
        ExprKind::Error { .. } => unknown("an error alternative"),
        ExprKind::Inherited => unknown("an inherited body"),
    }
}

fn alternatives(alternatives: &[Alternative]) -> Code {
    let each = alternatives.iter().map(|alternative| {
        if alternative.builder.is_some() {
            unknown("a value builder");
        }
        let code = Code::call("alt", vec![expr(&alternative.expr)]);
        match &alternative.alias {
            Some(alias) => code.then(&format!(".alias({:?})", alias.text)),
            None => code,
        }
    });
    Code::vec(each.collect())
}

/// The statement that adds a rule.
fn rule(rule: &Rule) -> Code {
    let name = Code::string(&rule.name.text);
    let added = match &rule.body {
        RuleBody::Alternatives(items) => {
            let plain = (items.iter()).all(|item| item.alias.is_none() && item.builder.is_none());
            match items.as_slice() {
                // `rule` makes alternatives of a choice at the top, so a lone alternative
                // that is a choice cannot go through it.
                [only] if plain && !matches!(only.expr.kind, ExprKind::Choice(_)) => {
                    Code::call("g.rule", vec![name, expr(&only.expr)])
                }
                [_, _, ..] if plain => {
                    let items = items.iter().map(|item| expr(&item.expr)).collect();
                    Code::call("g.rule", vec![name, Code::array("choice", items)])
                }
                _ => Code::call("g.rule_alternatives", vec![name, alternatives(items)]),
            }
        }
        RuleBody::Precedence(levels) => {
            let levels = levels.iter().map(|level| {
                let name = Code::string(&level.name.text);
                Code::call("level", vec![name, alternatives(&level.alternatives)])
            });
            Code::call("g.precedence", vec![name, Code::vec(levels.collect())])
        }
    };
    let mut settings = String::new();
    if rule.transparent {
        settings.push_str(".transparent()");
    }
    for attribute in &rule.attributes {
        settings.push_str(&format!(".attribute({:?})", attribute.text));
    }
    if let Some(display) = &rule.display {
        settings.push_str(&format!(".display({:?})", display.value));
    }
    if !rule.params.is_empty() {
        let names: Vec<String> = (rule.params.iter())
            .map(|param| format!("{:?}", param.text))
            .collect();
        settings.push_str(&format!(".params([{}])", names.join(", ")));
    }
    added.then(&settings)
}

/// The file `meta.rs` for a grammar of grammars read from `text`, whose licence header the
/// file takes over.
fn builder_code(text: &str, grammar: &Grammar) -> String {
    if grammar.tabs.is_some()
        || grammar.extends.is_some()
        || !(grammar.soft_keywords.is_empty()
            && grammar.imports.is_empty()
            && grammar.recovers.is_empty())
    {
        unknown("a directive other than %flag, %ignore and %word");
    }
    let mut statements = Vec::new();
    for flag in &grammar.flags {
        statements.push(Code::call("g.flag", vec![Code::string(&flag.text)]));
    }
    for ignored in &grammar.ignore {
        statements.push(Code::call("g.ignore", vec![expr(ignored)]));
    }
    if let Some(word) = &grammar.word {
        statements.push(Code::call("g.word", vec![expr(word)]));
    }
    statements.extend(grammar.rules.iter().map(rule));
    let mut out = String::new();
    for line in text.lines().take_while(|line| line.starts_with("// SPDX-")) {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(
        "\n\
         //! The grammar of grammars as builder code, which the front end reads grammars with.\n\
         //!\n\
         //! Generated. The tests print this file from `crates/fpeg/grammars/fpeg.fpeg` and hold it\n\
         //! to that, and `FPEG_BLESS=1 cargo test` rewrites it. Change the text, never this file.\n\
         \n\
         use super::Grammar;\n\
         use super::builder::*;\n\
         \n\
         /// The grammar of fpeg grammars.\n\
         pub(crate) fn grammar() -> Grammar {\n    \
             let mut g = GrammarBuilder::new();\n",
    );
    for statement in &statements {
        out.push_str(INDENT);
        statement.write(1, INDENT.len(), ";", &mut out);
        out.push_str(";\n");
    }
    out.push_str("    g.build()\n}\n");
    out
}

mod tests {
    use super::builder_code;
    use crate::Parser;
    use crate::grammar::{Grammar, meta, text};

    fn repository_file(path: &str) -> String {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");
        let text = std::fs::read_to_string(format!("{root}{path}")).expect("the file exists");
        // A checkout on Windows may end its lines otherwise.
        text.replace("\r\n", "\n")
    }

    fn grammar_file() -> (String, Grammar) {
        let text = repository_file("crates/fpeg/grammars/fpeg.fpeg");
        let grammar = Grammar::from_text(&text).unwrap_or_else(|e| panic!("{e:?}"));
        (text, grammar)
    }

    #[test]
    fn the_bootstrap_is_what_the_grammar_file_prints() {
        let (text, grammar) = grammar_file();
        let printed = builder_code(&text, &grammar);
        let path = "crates/fpeg/src/grammar/meta.rs";
        if std::env::var_os("FPEG_BLESS").is_some() {
            let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");
            std::fs::write(format!("{root}{path}"), &printed).expect("the file is written");
        }
        assert!(
            repository_file(path) == printed,
            "{path} is not what fpeg.fpeg prints: run FPEG_BLESS=1 cargo test, and review the diff"
        );
    }

    #[test]
    fn the_bootstrap_builds_the_grammar_of_the_grammar_file() {
        let (_, grammar) = grammar_file();
        assert_eq!(grammar.to_string(), meta::grammar().to_string());
    }

    #[test]
    fn the_parser_that_the_grammar_file_describes_reads_the_file_as_the_bootstrap_does() {
        let (text, read) = grammar_file();
        let described = Parser::new(&read).unwrap_or_else(|e| panic!("{e:?}"));
        let again = text::read(&described, &text, 0).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(read, again);
    }

    #[test]
    fn reference_page_shows_the_grammar_file() {
        let (file, _) = grammar_file();
        let body = file
            .split_once("\n\n%flag")
            .map(|(_, rest)| format!("%flag{rest}"));
        let page = repository_file("docs/reference/grammar.md");
        let block = page
            .split_once("## The grammar of grammars")
            .and_then(|(_, rest)| rest.split_once("```fpeg\n"))
            .and_then(|(_, rest)| rest.split_once("```"))
            .map(|(block, _)| block.to_owned());
        assert!(body.is_some() && body == block);
    }
}
