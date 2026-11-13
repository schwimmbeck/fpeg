// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! The grammar of grammars, built in code. It mirrors the text in `crates/fastpeg/grammars/fpeg.fpeg` and in
//! the language reference rule for rule, and a test holds the two together.

use super::Grammar;
use super::builder::*;

/// Characters of names after the first.
fn name_rest() -> crate::grammar::Expr {
    class([range('A', 'Z'), range('a', 'z'), range('0', '9'), ch('_')])
}

/// A list of `item` separated by commas, with line breaks ignored inside the brackets.
fn bracketed(
    open: &str,
    field_name: &str,
    item: crate::grammar::Expr,
    close: &str,
) -> crate::grammar::Expr {
    seq([
        lit(open),
        on(
            "nested",
            seq([field(field_name, sep_some(item, lit(","))), lit(close)]),
        ),
    ])
}

/// The grammar of fastpeg grammars.
pub(crate) fn grammar() -> Grammar {
    let mut g = GrammarBuilder::new();
    g.flag("nested");
    g.ignore(choice([
        some(class([ch(' '), ch('\t')])),
        r("COMMENT"),
        seq([if_flag("nested"), r("_NL")]),
    ]));
    let nl_bar = || seq([opt(r("_NL")), lit("|")]);

    g.rule(
        "start",
        seq([
            opt(r("_NL")),
            opt(seq([r("item"), many(seq([r("_NL"), r("item")]))])),
            opt(r("_NL")),
        ]),
    );
    g.rule("item", choice([r("rule"), r("directive")]))
        .transparent();

    g.rule(
        "rule",
        seq([
            field("attributes", many(r("attribute"))),
            field("transparent", opt(lit("?"))),
            field("name", r("NAME")),
            field("params", opt(r("params"))),
            field("display", opt(r("STRING"))),
            lit(":"),
            field("body", r("rule_body")),
        ]),
    );
    g.rule(
        "attribute",
        seq([lit("@"), field("name", r("NAME")), r("_NL")]),
    );
    g.rule("params", bracketed("<", "names", r("NAME"), ">"));
    g.rule("rule_body", choice([r("precedence"), r("alternatives")]))
        .transparent();

    g.rule(
        "precedence",
        seq([
            lit("%precedence"),
            lit("{"),
            r("_NL"),
            field("levels", some(seq([r("level"), r("_NL")]))),
            lit("}"),
        ]),
    );
    g.rule(
        "level",
        seq([
            field("name", r("NAME")),
            lit(":"),
            field("body", r("alternatives")),
        ]),
    );

    g.rule(
        "alternatives",
        seq([
            opt(nl_bar()),
            field("items", sep_some(r("alternative"), nl_bar())),
        ]),
    );
    g.rule(
        "alternative",
        seq([
            field("expr", r("sequence")),
            field("alias", opt(r("alias"))),
            field("builder", opt(r("builder"))),
        ]),
    );
    g.rule("alias", seq([lit("->"), field("name", r("NAME"))]));
    g.rule(
        "builder",
        seq([lit("=>"), field("value", r("builder_value"))]),
    );

    g.rule("choice", field("items", sep_some(r("sequence"), nl_bar())));
    g.rule("sequence", field("items", some(r("element"))));
    g.rule(
        "element",
        seq([
            field("expr", r("prefix")),
            field("label", opt(r("error_label"))),
        ]),
    );
    g.rule(
        "error_label",
        seq([lit("^"), field("value", choice([r("STRING"), r("NAME")]))]),
    );
    g.rule("prefix", choice([r("lookahead"), r("field"), r("suffix")]))
        .transparent();
    g.rule(
        "lookahead",
        seq([
            field("op", choice([lit("&"), lit("!")])),
            field("expr", r("prefix")),
        ]),
    );
    g.rule(
        "field",
        seq([
            field("name", r("NAME")),
            lit("="),
            not(lit(">")),
            field("expr", r("suffix")),
        ]),
    );
    g.rule(
        "suffix",
        seq([
            field("base", r("primary")),
            field("ops", many(r("suffix_op"))),
        ]),
    );
    g.rule(
        "suffix_op",
        choice([
            r("separated"),
            r("optional"),
            r("many"),
            r("some"),
            r("bounds"),
        ]),
    )
    .transparent();
    g.rule(
        "separated",
        seq([
            field("op", choice([lit("**"), lit("++")])),
            field("separator", r("primary")),
        ]),
    );
    g.rule("optional", lit("?"));
    g.rule("many", lit("*"));
    g.rule("some", lit("+"));
    g.rule(
        "bounds",
        seq([
            lit("{"),
            field("min", r("INTEGER")),
            field("comma", opt(lit(","))),
            field("max", opt(r("INTEGER"))),
            lit("}"),
        ]),
    );

    g.rule(
        "primary",
        choice([
            r("group"),
            r("LITERAL"),
            r("CLASS"),
            r("PROPERTY"),
            r("inherited"),
            r("any"),
            r("cut"),
            r("special"),
            r("reference"),
        ]),
    )
    .transparent();
    g.rule(
        "group",
        seq([
            lit("("),
            on("nested", seq([field("expr", opt(r("choice"))), lit(")")])),
        ]),
    );
    g.rule("inherited", lit("..."));
    g.rule("any", lit("."));
    g.rule("cut", lit("~"));
    g.rule(
        "reference",
        seq([field("name", r("NAME")), field("args", opt(r("args")))]),
    );
    g.rule("args", bracketed("<", "items", r("choice"), ">"));

    g.rule(
        "special",
        choice([
            r("flag_on"),
            r("flag_off"),
            r("flag_test"),
            r("indent"),
            r("same"),
            r("def"),
            r("ref"),
            r("error_alternative"),
        ]),
    )
    .transparent();
    let named_with_expr = |keyword: &str, name_field: &str| {
        seq([
            lit(keyword),
            lit("("),
            on(
                "nested",
                seq([
                    field(name_field, r("NAME")),
                    lit(","),
                    field("expr", r("choice")),
                    lit(")"),
                ]),
            ),
        ])
    };
    g.rule("flag_on", named_with_expr("%on", "flag"));
    g.rule("flag_off", named_with_expr("%off", "flag"));
    g.rule(
        "flag_test",
        seq([
            lit("%if"),
            lit("("),
            field("negated", opt(lit("!"))),
            field("flag", r("NAME")),
            lit(")"),
        ]),
    );
    g.rule(
        "indent",
        seq([
            lit("%indent"),
            lit("("),
            on("nested", seq([field("expr", r("choice")), lit(")")])),
        ]),
    );
    g.rule("same", lit("%same"));
    g.rule("def", named_with_expr("%def", "name"));
    g.rule(
        "ref",
        seq([lit("%ref"), lit("("), field("name", r("NAME")), lit(")")]),
    );
    g.rule(
        "error_alternative",
        seq([
            lit("%error"),
            lit("("),
            field("message", r("STRING")),
            lit(")"),
        ]),
    );

    g.rule(
        "directive",
        choice([
            r("ignore"),
            r("word"),
            r("flags"),
            r("tabs"),
            r("keywords"),
            r("import"),
            r("extends"),
            r("recover"),
        ]),
    )
    .transparent();
    g.rule("ignore", seq([lit("%ignore"), field("expr", r("choice"))]));
    g.rule("word", seq([lit("%word"), field("expr", r("choice"))]));
    g.rule(
        "flags",
        seq([lit("%flag"), field("names", some(r("NAME")))]),
    );
    g.rule("tabs", seq([lit("%tabs"), field("width", r("INTEGER"))]));
    g.rule(
        "keywords",
        seq([
            lit("%keywords"),
            lit("soft"),
            field("words", some(r("STRING"))),
        ]),
    );
    g.rule(
        "import",
        seq([
            lit("%import"),
            field("module", choice([r("NAME"), r("STRING")])),
            field("names", opt(r("import_names"))),
            field("prefix", opt(r("import_prefix"))),
        ]),
    );
    g.rule("import_names", bracketed("(", "items", r("NAME"), ")"));
    g.rule("import_prefix", seq([lit("as"), field("name", r("NAME"))]));
    g.rule(
        "extends",
        seq([lit("%extends"), field("module", r("STRING"))]),
    );
    g.rule(
        "recover",
        seq([
            lit("%recover"),
            field("label", r("NAME")),
            lit(":"),
            field("expr", r("choice")),
        ]),
    );

    g.rule(
        "builder_value",
        choice([r("call"), r("constant"), r("NAME")]),
    );
    g.rule(
        "call",
        seq([
            field("name", r("NAME")),
            lit("("),
            on(
                "nested",
                seq([
                    field("args", sep_many(r("builder_arg"), lit(","))),
                    lit(")"),
                ]),
            ),
        ]),
    );
    g.rule(
        "builder_arg",
        seq([
            opt(seq([field("key", r("NAME")), lit("=")])),
            field("value", choice([r("STAR"), r("constant"), r("NAME")])),
        ]),
    );
    g.rule(
        "constant",
        field("value", choice([r("STRING"), r("NUMBER")])),
    );

    g.rule(
        "NAME",
        seq([
            class([range('A', 'Z'), range('a', 'z'), ch('_')]),
            many(name_rest()),
        ]),
    );
    g.rule("INTEGER", some(class([range('0', '9')])));
    g.rule(
        "NUMBER",
        seq([
            opt(lit("-")),
            some(class([range('0', '9')])),
            opt(seq([lit("."), some(class([range('0', '9')]))])),
        ]),
    );
    g.rule("STAR", lit("*"));
    let quoted = |quote: char| {
        let q = quote.to_string();
        seq([
            lit(&q),
            many(choice([
                seq([lit("\\"), any()]),
                not_class([ch(quote), ch('\\'), ch('\r'), ch('\n')]),
            ])),
            lit(&q),
        ])
    };
    g.rule("STRING", choice([quoted('"'), quoted('\'')]));
    g.rule(
        "LITERAL",
        seq([r("STRING"), opt(seq([lit("i"), not(name_rest())]))]),
    );
    g.rule(
        "CLASS",
        seq([
            lit("["),
            some(choice([
                seq([lit("\\"), any()]),
                not_class([ch(']'), ch('\\'), ch('\r'), ch('\n')]),
            ])),
            lit("]"),
        ]),
    );
    g.rule(
        "PROPERTY",
        seq([
            lit("\\"),
            class([ch('p'), ch('P')]),
            lit("{"),
            some(class([range('A', 'Z'), range('a', 'z'), ch('_')])),
            lit("}"),
        ]),
    );
    g.rule(
        "COMMENT",
        seq([lit("//"), many(not_class([ch('\r'), ch('\n')]))]),
    );
    g.rule(
        "_NL",
        some(seq([
            choice([lit("\r\n"), lit("\n"), lit("\r")]),
            many(class([ch(' '), ch('\t')])),
            opt(r("COMMENT")),
        ])),
    );
    g.build()
}

#[cfg(test)]
mod tests {
    use super::grammar;
    use crate::grammar::Grammar;

    fn repository_file(path: &str) -> String {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");
        std::fs::read_to_string(format!("{root}{path}")).expect("the file exists")
    }

    #[test]
    fn grammar_text_and_builder_print_the_same() {
        let text = repository_file("crates/fastpeg/grammars/fpeg.fpeg");
        let parsed = Grammar::from_text(&text).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(parsed.to_string(), grammar().to_string());
    }

    #[test]
    fn reference_page_shows_the_grammar_file() {
        let file = repository_file("crates/fastpeg/grammars/fpeg.fpeg");
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
