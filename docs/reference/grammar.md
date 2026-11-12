# Grammar language

This page is the normative reference for the syntax of fastpeg grammars, draft 0. What a grammar means is defined on the [semantics](semantics.md) page. Where this page and the code disagree, one of them has a bug.

Some constructs are part of the language from the start but take effect in a later milestone. Their status is marked where they are described.

## A first grammar

```fpeg
?start: value

?value: object
      | array
      | STRING
      | NUMBER
      | "true"
      | "false"
      | "null"

object: "{" (pair ** ",") "}"
pair: key=STRING ":" value=value
array: "[" (value ** ",") "]"

STRING: "\"" ([^"\\] | "\\" .)* "\""
NUMBER: "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+)? ([eE] [+-]? [0-9]+)?

%ignore [ \t\r\n]+
```

Lowercase names are syntactic rules, uppercase names are terminals. Ignored text such as whitespace may appear before every token of a syntactic rule. A terminal is matched as one token, with nothing skipped inside it.

## Lexical structure

A grammar is UTF-8 text.

- **Line breaks** are `\n`, `\r\n` and `\r`. A line break ends a rule or a directive, with two exceptions: inside parentheses `( )` and angle brackets `< >`, line breaks are ignored, and a line whose first token is `|` continues the rule above it.
- **Whitespace** (spaces and tabs) separates tokens and is otherwise ignored.
- **Comments** start with `//` and run to the end of the line.
- **Names** match `[A-Za-z_][A-Za-z0-9_]*`. After any leading underscores, a name that starts with a lowercase letter names a syntactic rule, one that starts with an uppercase letter names a terminal. The rest of the name may mix letters, digits and underscores, but a rule name written `Mixed` is a terminal, so keep rule names lowercase and terminal names uppercase.
- **Numbers** are decimal: `[0-9]+`. Value builders also accept a sign and a fraction.
- **Keywords** of the grammar language exist only as directives and forms that start with `%`, and attributes that start with `@`. Every plain name is free for rules.

### Literals

A literal is text between double quotes `"..."` or single quotes `'...'`. Both forms mean the same. A literal may not be empty and may not contain a line break.

| Escape | Meaning |
|---|---|
| `\\` `\"` `\'` | backslash and quotes |
| `\n` `\r` `\t` `\0` | line feed, carriage return, tab, NUL |
| `\xHH` | the code point with this two-digit hexadecimal value |
| `\u{H...}` | the code point with this hexadecimal value, one to six digits, not a surrogate |

Any other character after a backslash is an error.

A literal followed directly by `i`, as in `"select"i`, matches ASCII letters in either case. Other characters still match exactly.

### Character classes

A character class matches one code point.

```fpeg
DIGIT: [0-9]
HEX: [0-9a-fA-F]
NOT_QUOTE: [^"\\]
IDENT_START: [\p{XID_Start}_]
```

- `[...]` lists characters and ranges `a-z`. A range must not run backwards.
- `[^...]` matches any code point not listed.
- Inside a class, `]`, `\` and `^` at the start are written with a backslash. `-` stands for itself at the start or the end of a class, or as `\-`.
- Escapes are those of literals, plus `\[`, `\]`, `\-`, `\^`, and the Unicode properties below.
- `\p{Name}` matches a code point with the property, `\P{Name}` one without it. Both may also appear outside a class, as an expression of their own.
- `.` matches any one code point.

Supported properties: `XID_Start`, `XID_Continue`, `Alphabetic`, `White_Space`, `Uppercase`, `Lowercase`. Any other name is an error.

## Structure of a grammar

A grammar is a sequence of rules and directives, one per logical line. Their order does not matter, except that a parse starts at the rule named `start` unless the caller names another rule.

## Rules

```text
@attribute
name<params> "display name": alternative | alternative
```

Only the name and the body are required.

| Form | Meaning |
|---|---|
| `name: e` | a syntactic rule. Ignored text is skipped before each of its tokens. It produces a node |
| `NAME: e` | a terminal. Nothing is skipped inside it. It produces one token |
| `@lexical` before `name: e` | a lexical rule. It produces a node with children, like a syntactic rule, but nothing is skipped between its elements. Syntactic rules it calls skip as usual |
| `_name: e` | an inlined rule: its children join the parent's children |
| `_NAME: e` | a hidden terminal: it is matched and recorded, but views do not show it |
| `?name: e` | a transparent rule: when it has exactly one visible child, it is inlined. A transparent rule must not contain fields |
| `name "number": e` | a display name, used in error messages in place of the name |

A terminal may use literals, classes, Unicode properties, `.` and other terminals, but no syntactic or lexical rules.

### Attributes

Attributes stand on their own lines directly above a rule.

| Attribute | Meaning | Status |
|---|---|---|
| `@lexical` | the rule is a lexical rule | now |
| `@reserved` | the terminal never matches a reserved word (see [Words and keywords](#words-and-keywords)) | now |
| `@sync` | error recovery may resume before this rule or terminal | reserved, M8 |
| `@memo` | always memoize this rule | reserved, M4 |
| `@nomemo` | never memoize this rule | reserved, M4 |

An unknown attribute is an error.

### Parameters

```fpeg
list<item, separator>: item (separator item)*
args: "(" list<expr, ","> ")"
```

A rule with parameters is a template. Parameters are names that stand for expressions inside the body. Each use with a distinct list of arguments becomes a rule of its own, with the same name, before anything is matched. The arguments are expressions, and a use must pass as many as the rule declares.

## Expressions

From loosest to tightest:

| Form | Meaning |
|---|---|
| `e1 \| e2` | ordered choice: try `e1`, and `e2` only if `e1` fails |
| `e1 e2` | sequence |
| `e ^ "message"`, `e ^ label` | error label: if `e` fails, report this error (see [Errors](#errors)) |
| `&e`, `!e` | positive and negative lookahead: test `e` without consuming input |
| `label=e` | field: `e` becomes a named part of the node (see [Fields](#fields)) |
| `e?` `e*` `e+` | optional, zero or more, one or more |
| `e{n}` `e{n,}` `e{n,m}` | exactly `n`, at least `n`, between `n` and `m` repetitions |
| `e ** s`, `e ++ s` | zero or more, and one or more, `e` separated by `s` |
| primary | a literal, a class, a property, `.`, a rule name, `( e )`, `()`, `~` or a `%` form |

- Repetition is greedy and never gives back what it matched.
- In `e ** s` and `e ++ s` the separator `s` is a primary, so a compound separator needs parentheses. A trailing separator is not matched.
- `()` matches the empty string.
- `~` is a cut: once the sequence it stands in has passed it, a failure later in that sequence makes the innermost choice or repetition fail at once, without trying its remaining alternatives.

## Shaping the tree

A successful parse produces a tree of nodes and tokens. Every token keeps its exact position in the input, and the tree is lossless: every matched character belongs to a token, except text the `%ignore` expression skipped, which lies between tokens.

### Nodes and tokens

- A syntactic or lexical rule produces a node whose kind is the rule's name. Its children are what its body produces.
- A token is produced by each terminal used in a syntactic or lexical rule, and by each literal, class, property or `.` written in one. Inside a terminal nothing produces a token of its own: the terminal's whole match is one token.
- Tokens of literals, classes, properties, `.` and hidden terminals `_NAME` are hidden. Views of the tree skip hidden tokens, but the tree keeps them.

### Aliases

`-> name` after an alternative of a rule names the node that alternative produces.

```fpeg
?atom: NUMBER
     | "-" atom   -> negative
     | "(" expr ")"
```

### Fields

`label=e` gives `e` a name in the node, so that code can read it by name instead of by position.

```fpeg
pair: key=STRING ":" value=value
call: func=NAME "(" args=(expr ** ",") ")"
binary: left=term op=("+" | "-") right=term
```

- A field makes the tokens written directly inside it visible, as `op` above shows.
- The cardinality of a field is fixed by the grammar: one value for `x=e`, a value or none for `x=e?`, a list for `x=e*`, `x=e+`, `x=e{n,m}`, a separated repetition, or a label used more than once in the rule.
- A field captures one item per match. A field whose expression may produce several visible items in one match, such as `x=(A B)`, is an error.

### Spans

A node spans from the start of its first token to the end of its last token, hidden tokens included. Skipped text never widens a span. A node without tokens has an empty span at the position where its rule started matching. Spans are byte offsets into the UTF-8 input.

### Operator precedence

A precedence block lists operator levels from loosest to tightest. Each level is named and may refer to itself, to tighter levels and to looser levels by name.

```fpeg
expr: %precedence {
    sum: left=sum op=("+" | "-") right=product   -> binary
    product: left=product op=("*" | "/") right=unary   -> binary
    unary: op="-" operand=unary   -> unary
    power: base=atom "**" exponent=unary   -> power
    atom: NUMBER | NAME | "(" expr ")"
}
```

- Each level tries its own alternatives first. If none matches, it falls through to the next tighter level, and the fall-through adds no node.
- An alternative without `->` adds no node, so `"(" expr ")"` yields the inner expression. An alternative with fields must have `->`.
- An operator whose left operand is its own level, as in `left=sum ... right=product`, is left associative. One whose right operand is its own level or a looser one is right associative: `exponent=unary` names the looser level `unary`, so `2 ** -1` parses and `2 ** 3 ** 2` groups to the right.
- The rule itself produces no node. Level names are local to the block and may not shadow a rule of the grammar.
- The block means one rule per level, with left recursion. An engine may parse it differently as long as the results agree.

## Ignored text

```fpeg
%ignore [ \t\r\n]+
%ignore "//" [^\n]*
```

The skip expression is the ordered choice of all `%ignore` expressions. It is applied before every token of a syntactic rule, before the end of the input, and before the column checks of `%indent` and `%same` in syntactic rules. It is repeated as long as it matches a non-empty text. It may test flags. It produces no tokens and must not use syntactic or lexical rules.

A grammar without `%ignore` skips nothing.

## Words and keywords

| Directive | Meaning |
|---|---|
| `%word e` | the word characters, usually a class. Default `[A-Za-z0-9_]` |
| `%keywords soft "match" "case"` | literals that remain valid identifiers |

- A literal in a syntactic or lexical rule whose last character is a word character does not match when a word character follows. `"if"` never matches the start of `iffy`.
- The reserved words are the literals of the syntactic and lexical rules that consist only of word characters, minus the soft keywords. A terminal marked `@reserved` fails where its match is a reserved word. Case-insensitive literals reserve every spelling.

## Context

Three small mechanisms let a grammar depend on what surrounds a match. None of them runs user code.

### Flags

```fpeg
%flag bracket
%ignore [ \t]+ | %if(bracket) "\n"

group: "(" %on(bracket, expr ")")
```

| Form | Meaning |
|---|---|
| `%flag a b` | declare flags |
| `%on(a, e)`, `%off(a, e)` | match `e` with the flag set or cleared |
| `%if(a)`, `%if(!a)` | succeed without consuming input when the flag is set, or cleared |

A flag holds for the whole extent of `e`, including the rules it calls and the skip expression. Using an undeclared flag is an error. The closing `")"` above is inside `%on`, because skipping happens before a token: a line break in front of `)` is skipped only while the flag is set.

### Indentation

```fpeg
block: NEWLINE %indent((%same stmt)+)
```

| Form | Meaning |
|---|---|
| `%indent(e)` | the next token must start right of the reference column. Its column becomes the reference column inside `e` |
| `%same` | the next token must start exactly at the reference column. Consumes nothing |
| `%tabs n` | a tab advances to the next multiple of `n` columns when columns are compared. Default 8 |

The reference column is 0 at the start of a parse. Columns count code points from the start of the line, starting at 0.

### Captures

```fpeg
RAW_STRING: "r" %def(hashes, "#"*) "\"" (!("\"" %ref(hashes)) .)* "\"" %ref(hashes)
```

| Form | Meaning |
|---|---|
| `%def(name, e)` | match `e` and remember the matched text under the name |
| `%ref(name)` | match exactly the text last remembered under the name |

A capture lasts until the rule activation that made it returns, and rules called from there see it. `%ref` fails when nothing is remembered under the name.

## Errors

| Form | Meaning | Status |
|---|---|---|
| `e ^ "message"` | if `e` fails, stop the parse with this message | now |
| `e ^ label` | if `e` fails, stop the parse with the named error | now |
| `%recover label: e` | when the named error occurs, `e` skips to a place where parsing resumes | M8 |
| `%error("message")` at the end of an alternative | an error alternative, tried only when a failed parse is explained | M8 |
| `@sync` | recovery may resume before this rule | M8 |

## Values

`=> builder` after an alternative says how to turn the node into a value when no Python method handles it. Builders never affect parsing. Status: syntax now, effect with the Python binding (M3).

| Builder | Result |
|---|---|
| `=> int`, `=> float`, `=> text`, `=> unquote` | a value from a token |
| `=> list`, `=> tuple`, `=> dict` | a value from the visible children |
| `=> None`, `=> True`, `=> False`, a number, a string | a constant |
| `=> Name(a, b, key=c, items=*, flag=None)` | a call of a registered function with fields, constants and `*` for all children |

## Modules

Status: syntax now, effect in M7.

| Form | Meaning |
|---|---|
| `%import common (WS, INT)` | take the named rules from another grammar, with what they depend on |
| `%import python as py` | take a whole grammar under a prefix |
| `%extends "python"` | start from another grammar. A rule defined again replaces the inherited one |
| `...` | inside a replacing rule, the inherited body |

## The grammar of grammars

The syntax above, written in the language itself. The text front end is built from exactly this grammar.

```fpeg
%flag nested
%ignore [ \t]+ | COMMENT | %if(nested) _NL

start: _NL? (item (_NL item)*)? _NL?
?item: rule | directive

rule: attributes=attribute* transparent="?"? name=NAME params=params? display=STRING? ":" body=rule_body
attribute: "@" name=NAME _NL
params: "<" %on(nested, names=(NAME ++ ",") ">")
?rule_body: precedence | alternatives

precedence: "%precedence" "{" _NL levels=(level _NL)+ "}"
level: name=NAME ":" body=alternatives

alternatives: (_NL? "|")? items=(alternative ++ (_NL? "|"))
alternative: expr=sequence alias=alias? builder=builder?
alias: "->" name=NAME
builder: "=>" value=builder_value

choice: items=(sequence ++ (_NL? "|"))
sequence: items=element+
element: expr=prefix label=error_label?
error_label: "^" value=(STRING | NAME)
?prefix: lookahead | field | suffix
lookahead: op=("&" | "!") expr=prefix
field: name=NAME "=" !">" expr=suffix
suffix: base=primary ops=suffix_op*
?suffix_op: separated | optional | many | some | bounds
separated: op=("**" | "++") separator=primary
optional: "?"
many: "*"
some: "+"
bounds: "{" min=INTEGER comma=","? max=INTEGER? "}"

?primary: group | LITERAL | CLASS | PROPERTY | inherited | any | cut | special | reference
group: "(" %on(nested, expr=choice? ")")
inherited: "..."
any: "."
cut: "~"
reference: name=NAME args=args?
args: "<" %on(nested, items=(choice ++ ",") ">")

?special: flag_on | flag_off | flag_test | indent | same | def | ref | error_alternative
flag_on: "%on" "(" %on(nested, flag=NAME "," expr=choice ")")
flag_off: "%off" "(" %on(nested, flag=NAME "," expr=choice ")")
flag_test: "%if" "(" negated="!"? flag=NAME ")"
indent: "%indent" "(" %on(nested, expr=choice ")")
same: "%same"
def: "%def" "(" %on(nested, name=NAME "," expr=choice ")")
ref: "%ref" "(" name=NAME ")"
error_alternative: "%error" "(" message=STRING ")"

?directive: ignore | word | flags | tabs | keywords | import | extends | recover
ignore: "%ignore" expr=choice
word: "%word" expr=choice
flags: "%flag" names=NAME+
tabs: "%tabs" width=INTEGER
keywords: "%keywords" "soft" words=STRING+
import: "%import" module=(NAME | STRING) names=import_names? prefix=import_prefix?
import_names: "(" %on(nested, items=(NAME ++ ",") ")")
import_prefix: "as" name=NAME
extends: "%extends" module=STRING
recover: "%recover" label=NAME ":" expr=choice

builder_value: call | constant | NAME
call: name=NAME "(" %on(nested, args=(builder_arg ** ",") ")")
builder_arg: (key=NAME "=")? value=(STAR | constant | NAME)
constant: value=(STRING | NUMBER)

NAME: [A-Za-z_] [A-Za-z0-9_]*
INTEGER: [0-9]+
NUMBER: "-"? [0-9]+ ("." [0-9]+)?
STAR: "*"
STRING: "\"" ("\\" . | [^"\\\r\n])* "\"" | "'" ("\\" . | [^'\\\r\n])* "'"
LITERAL: STRING ("i" ![A-Za-z0-9_])?
CLASS: "[" ("\\" . | [^\]\\\r\n])+ "]"
PROPERTY: "\\" [pP] "{" [A-Za-z_]+ "}"
COMMENT: "//" [^\r\n]*
_NL: (("\r\n" | "\n" | "\r") [ \t]* COMMENT?)+
```
