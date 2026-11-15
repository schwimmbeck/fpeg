# Grammar language

This page is the normative reference for the syntax of fpeg grammars, draft 0. What a grammar means is defined on the [semantics](semantics.md) page. Where this page and the code disagree, one of them has a bug.

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
| `?name: e` | a transparent rule: when it has exactly one visible child, it is inlined. Fields are allowed only in its aliased alternatives, which always make a node. An alternative that is inlined for some inputs only gets a warning ([W301](diagnostics/W301.md)) |
| `name "number": e` | a display name, used in error messages in place of the name |

A terminal may use literals, classes, Unicode properties, `.` and other terminals, but no syntactic or lexical rules.

### Attributes

Attributes stand on their own lines directly above a rule.

| Attribute | Meaning | Status |
|---|---|---|
| `@lexical` | the rule is a lexical rule | now |
| `@reserved` | the terminal never matches a reserved word (see [Words and keywords](#words-and-keywords)) | now |
| `@sync` | error recovery may resume before this rule or terminal | reserved, M8 |
| `@memo` | remember the results of this rule (see [Remembered results](#remembered-results)) | now |
| `@nomemo` | never remember the results of this rule | now |

An unknown attribute is an error.

### Remembered results

A parser may try a rule at one position of the input more than once, when an alternative that used the rule fails and the next one starts with it again. A rule marked `@memo` is matched once at each position, and every later call there gets the stored result.

```fpeg
start: dot | bang | item
dot: item "."
bang: item "!"
@memo
item: "(" start ")" | "x"
```

- Remembering never changes what a grammar matches or the tree it gives, only the work a parse takes. It pays for a rule that is tried again and again and takes long to match, and costs a lookup for every call otherwise.
- A rule that a choice tries twice at one position is remembered without a mark: one that two alternatives of a choice start with, as `item` in `start: item "." | item "!" | item`, or a part that can end after a failed try and the part after it, as in `item? item`. Only the rules that a choice starts with itself count, not those they call in turn, as `item` above through `dot` and `bang`, and terminals do not.
- A left-recursive rule is remembered without a mark where each pass over it tries its other alternatives again, as the last pass, which finds that the match grows no further, does. A rule whose alternatives start with the rule itself before the others, as the levels of a [precedence block](#operator-precedence) do, runs as a loop that tries them once and is not.
- `@nomemo` turns remembering without a mark off for a rule, and on a block for each of its levels. `@memo` on a block changes nothing.
- No other rule is remembered unless it is marked. On a rule with parameters a mark holds for every use.
- A rule cannot be `@memo` and `@nomemo` at once ([E302](diagnostics/E302.md)).

### Parameters

```fpeg
list<item, separator>: item (separator item)*
args: "(" list<expr, ","> ")"
```

A rule with parameters is a template. Parameters are names that stand for expressions inside the body. Each use with a distinct list of arguments becomes a rule of its own, with the same name, before anything is matched. The arguments are expressions, and a use must pass as many as the rule declares.

### Left recursion

A rule may call itself at the very place where it started, directly or through other rules.

```fpeg
start: chain
?chain: chain "." NAME -> member
      | NAME
NAME: [a-z]+
```

- Such a rule matches as far as it can and groups to the left: `a.b.c` is the member `c` of the member `b` of `a`. The [semantics](semantics.md) page says exactly how.
- A rule that calls itself at both ends, as in `e: e "-" e | N`, groups to the right.
- For operators, a [precedence block](#operator-precedence) says the same with less to write.
- A chain of a thousand members gives a tree a thousand nodes deep.

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
- `**` and `++` must be followed by their separator. A repetition of a repetition is written `(e*)*` or `e* *`.
- `()` matches the empty string.
- Expressions nest at most 100 levels deep, counting every operator and group. A deeper grammar is an error. So are more than 1000 instances of templates, and instances that expand into more than 100,000 expressions together, counting a copy of an argument for every use of its parameter. A grammar takes in at most 1000 [modules](#modules) together, counting one again for each directive that takes it in, at most 64 deep in a chain of grammars that extend or import the next, and with at most 64 MiB of text.
- `~` is a cut: once the sequence it stands in has passed it, a failure later in that sequence makes the innermost choice or repetition fail at once, without trying its remaining alternatives.

## Shaping the tree

A successful parse produces a tree of nodes and tokens. Every token keeps its exact position in the input, and the tree is lossless: every matched character belongs to a token, except text the `%ignore` expression skipped, which lies between tokens.

### Nodes and tokens

- A syntactic or lexical rule produces a node whose kind is the rule's name. Its children are what its body produces.
- A token is produced by each terminal used in a syntactic or lexical rule, and by each literal, class, property, `.` or `%ref` written in one. Inside a terminal nothing produces a token of its own: the terminal's whole match is one token.
- Tokens of literals, classes, properties, `.`, `%ref` and hidden terminals `_NAME` are hidden. Views of the tree skip hidden tokens, but the tree keeps them.

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

- A field captures the token of a literal, class, property, `.`, `%ref` or hidden terminal when that token is the field's whole match, as `op` above shows: directly, or through choices, repetitions, `^`, `%on`, `%off`, `%indent` and `%def`. Tokens that stand in a sequence with others, a separator or a lookahead stay hidden, so `args=(expr ** ",")` holds the expressions and not the commas. Hidden tokens never carry a field, so the commas are not labelled `args` either.
- The cardinality of a field is fixed by the grammar: one value for `x=e`, a value or none for `x=e?`, a list for `x=e*`, `x=e+`, `x=e{n,m}`, a separated repetition, or a label used more than once in the rule.
- A field captures one item per match. A field whose expression may produce several visible items in one match, such as `x=(A B)`, is an error.
- A rule without a node of its own passes its fields on to the node around it, together with those the rules it calls pass on. One rule may pass on at most 1000 fields ([E402](diagnostics/E402.md)).

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
- The rule itself produces no node.
- A level name belongs to the whole grammar. Another rule may call a level by its name and gets an operand of that level or a tighter one, as `call: func=atom "(" args=(sum ** ",") ")"` would. No rule and no other level may have the name of a level.
- The block means one rule per level, with left recursion. An engine may parse it differently as long as the results agree.

## Ignored text

```fpeg
%ignore [ \t\r\n]+
%ignore "//" [^\n]*
```

The skip expression is the ordered choice of all `%ignore` expressions. It is applied before every token of a syntactic rule, before the end of the input, and before the column checks of `%indent` and `%same` in syntactic rules. It is repeated as long as it matches a non-empty text. It may test flags, check the column against the reference column, and match captures with `%ref`. What it captures itself holds for that one match. It produces no tokens and must not use syntactic or lexical rules.

A grammar without `%ignore` skips nothing.

## Words and keywords

| Directive | Meaning |
|---|---|
| `%word e` | the word characters, a class, and word boundaries for literals |
| `%keywords soft "match" "case"` | literals that remain valid identifiers |

- In a grammar with `%word`, a literal in a syntactic or lexical rule whose last character is a word character does not match when a word character follows. `"if"` never matches the start of `iffy`. Without `%word`, literals match as written.
- The reserved words are the literals of the syntactic and lexical rules that consist only of word characters, minus the soft keywords. Without `%word`, the word characters for this purpose are `[A-Za-z0-9_]`. A terminal marked `@reserved` fails where its match is a reserved word. Case-insensitive literals reserve every spelling.

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

A flag holds for the whole extent of `e`, including the rules it calls and the skip expression. Using an undeclared flag is an error, and a grammar declares at most 64 flags. The closing `")"` above is inside `%on`, because skipping happens before a token: a line break in front of `)` is skipped only while the flag is set.

### Indentation

```fpeg
block: NEWLINE %indent((%same stmt)+)
```

| Form | Meaning |
|---|---|
| `%indent(e)` | the next token must start right of the reference column. Its column becomes the reference column inside `e` |
| `%same` | the next token must start exactly at the reference column. Consumes nothing |
| `%tabs n` | a tab advances to the next multiple of `n` columns when columns are compared. Default 8 |

The reference column is 0 at the start of a parse. Columns count code points from the start of the line, starting at 0, and a form feed sets the count back to 0. A failed check expects indentation, at the position it checked.

In the skip expression a check compares with the reference column of the token the skip stands before, so a line break can be ignored where the next line starts right of that column, which continues a block's item on the next line:

```fpeg
%ignore " "+ | "\n" " "* %indent(())
```

### Captures

```fpeg
RAW_STRING: "r" %def(hashes, "#"*) "\"" (!("\"" %ref(hashes)) .)* "\"" %ref(hashes)
```

| Form | Meaning |
|---|---|
| `%def(name, e)` | match `e` and remember the matched text under the name. In a syntactic rule the text starts after ignored text |
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

A label stops the parse however `e` fails, also after a cut inside `e`. The error points at the start of `e`, after any ignored text.

## Values

`=> builder` after an alternative says how to turn its node or token into a value when no method of a transformer handles it. Builders never affect parsing.

| Builder | Value |
|---|---|
| `=> int`, `=> float` | Python's `int` or `float` of the text: a token's text, or the input a node spans |
| `=> text` | the text as a plain string |
| `=> unquote` | the text without its first and last character, its quotes, with the escapes of JSON decoded and `\'` too: `\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t` and `\uXXXX`, where two escapes of a surrogate pair make one character and a lone surrogate stays one. Another escape is an error |
| `=> list`, `=> tuple`, `=> dict` | of the values of the visible children, `dict` from pairs |
| `=> None`, `=> True`, `=> False`, a number, a string | the constant, a number with a fraction being a float |
| `=> Name(a, b, key=c, items=*, flag=None)` | a call of the function `Name` given with the grammar: a field gives its value, or `None` when absent, for a field of at most one item and a list for a field of several, a constant gives itself and `*` the list of the values of all visible children |
| `=> Name` alone | the function `Name` given with the grammar, called with the text of a token or the list of a node's children, as `int` and `list` are |

- A builder belongs to its alternative. Each alternative with a builder makes nodes of a kind of their own, named like the rule or the alias, so a tree reads the same with builders and without. Alternatives with equal builders share that kind, across rules too.
- A builder needs a node to make a value of. An alternative of a rule `_name`, or an alternative without `->` in a precedence level, makes none ([E309](diagnostics/E309.md)). In a transparent rule `?name` the builder applies when the node stays, and an alternative that gives way to its one visible child leaves its builder unused, which is a warning where it always gives way ([W302](diagnostics/W302.md)).
- On a terminal a builder applies to every token of it, so the terminal's alternatives give one builder. A token has no children and no fields, so `list`, `tuple`, `dict`, `*` and fields do not fit there ([E309](diagnostics/E309.md)).
- A field a builder names must be one its alternative can hold, written in it or given by a rule it calls that makes no node of its own ([E207](diagnostics/E207.md)).
- The built-in builders take no arguments, a keyword is given once, and positional arguments come before keywords ([E310](diagnostics/E310.md)).
- Every function the builders call is given with the grammar when it is loaded ([E208](diagnostics/E208.md)).

## Modules

A grammar can start from another one and change it, and take single rules from others. The grammars that come with fpeg are `common`, `json` and `python`. The [guide to composing grammars](../guide/composing-grammars.md) shows each form at work.

```fpeg
%extends "json"
%import common (CPP_COMMENT)
?value: ... | NAME
NAME: [a-z]+
%ignore CPP_COMMENT
```

| Form | Meaning | Status |
|---|---|---|
| `%extends "json"` | start from the rules and directives of another grammar | now |
| `...` | in a rule that replaces an inherited one, the inherited alternatives | now |
| `%import common (WS, INT)` | take the named rules from another grammar, with what they depend on | now |
| `%import python as py` | take a whole grammar under a prefix | M9 |

- A rule defined again replaces the inherited one for every reference, also in the inherited rules. Replacing `?value` above lets every array and member hold a name.
- `...` stands as a whole alternative, at most once, and puts the inherited alternatives there, with their aliases and builders: `?value: ... | NAME` tries them first, `?value: NAME | ...` last. A rule that uses it takes the parameters of the rule it replaces ([E605](diagnostics/E605.md)). In a precedence block, or for one, it waits for M9 ([E501](diagnostics/E501.md)).
- A rule that replaces another says its own markers and attributes, `?` and `@memo` included.
- The `%ignore` expressions of the grammar come after those of the one it extends, its flags and soft keywords join the other's, and its `%word` and `%tabs`, where it gives them, replace the other's.
- A grammar extends at most one other ([E204](diagnostics/E204.md)), one that exists ([E601](diagnostics/E601.md)), which may extend another in turn.
- The grammars it extends and imports come from the modules given with it where it is loaded, then from those that come with fpeg. A grammar that takes itself in through them is an error ([E602](diagnostics/E602.md)).
- An import takes the rules it names under their names, and the rules they need under the grammar's name and a dot, as `common.DIGIT`, which no grammar can write, so they clash with none. Each keeps its kind, so `common.DIGIT` is a terminal as `DIGIT` is, and its nodes and tokens carry that name, as do the aliases of its alternatives. A name that an imported rule refers to and its grammar lacks is an error there ([E201](diagnostics/E201.md)), and never takes a rule of the grammar that imports it. The levels of a precedence block taken in come under such names too, also those of a block taken by name. Such a name comes from one rule of one module, so a copy that the grammar has already, through what it extends or another import, is shared. `%import common` without names takes every rule of `common` by name, and those it needs from the grammars `common` imports under private names. A module name is plain: a letter, then letters, digits and underscores.
- A rule the grammar defines too cannot be imported ([E603](diagnostics/E603.md)), nor one the other grammar lacks ([E604](diagnostics/E604.md)).
- The flags that the rules imported set or test join the grammar's under their names, so its skip expression can test one they set. Their grammar's other directives stay behind: the grammar that imports says what it skips and what its words are.
- A problem that `check` finds in the rules of a grammar extended or imported is reported at `%extends` or `%import`, with the line and column in that grammar.

`common` holds terminals that many grammars need: `DIGIT`, `HEXDIGIT`, `INT`, `SIGNED_INT`, `DECIMAL`, `FLOAT`, `SIGNED_FLOAT`, `NUMBER`, `SIGNED_NUMBER`, `ESCAPED_STRING`, `LCASE_LETTER`, `UCASE_LETTER`, `LETTER`, `WORD`, `CNAME`, `WS_INLINE`, `WS`, `CR`, `LF`, `NEWLINE`, `SH_COMMENT`, `CPP_COMMENT`, `C_COMMENT` and `SQL_COMMENT`.

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
separated: op=("**" | "++") separator=primary ^ "a separator after ** or ++"
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
