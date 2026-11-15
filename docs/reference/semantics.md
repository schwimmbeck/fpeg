# Semantics

This page defines what a grammar means: for a grammar, an input and a start rule, which tree a parse produces or where it fails. It is normative, draft 0. The [grammar language](grammar.md) page defines the syntax this page refers to.

The definition has two steps. First, every grammar is translated into a small core language. Then a single matching relation gives every core expression its meaning. Engines and optimizations may work in any way, as long as they produce the results defined here.

## Input

- The input is a UTF-8 string. Positions are byte offsets from 0 to the length of the input, and every position used below lies on a code point boundary.
- A *character* is one code point.
- A *line break* is `\r\n`, `\n` or `\r`. A line starts at position 0 and after every line break.
- The *column* of a position is computed over the characters between the start of its line and the position: each character advances the column by 1, a tab advances it to the next multiple of the tab width (`%tabs`, default 8), and a form feed sets it back to 0. The first column is 0.

## The core language

| Core expression | Meaning in short |
|---|---|
| `ε` | match the empty string |
| `lit(s)`, `lit_i(s)` | match the string `s`, the second ignoring ASCII case |
| `class(C)` | match one character in the set `C` |
| `any` | match one character |
| `call(R)` | match rule `R` |
| `seq(e1, ..., en)` | match each in turn |
| `choice(e1, ..., en)` | ordered choice |
| `rep(e, n, m)` | between `n` and `m` matches of `e`, `m` possibly unbounded |
| `not(e)`, `and(e)` | lookahead |
| `cut` | commit the innermost choice point |
| `skip` | match the skip expression as often as it matches non-empty text |
| `token(k, h, e)` | match `e` as one token of kind `k`, hidden if `h` |
| `node(k, e)` | wrap what `e` produces in a node of kind `k` |
| `transparent(k, e)` | as `node`, unless `e` produces exactly one visible item |
| `field(f, e)` | label what `e` produces with field `f` |
| `word_end` | fail when a word character follows |
| `unreserved(e)` | match `e`, fail when its text is a reserved word |
| `on(a, e)`, `off(a, e)`, `if(a)`, `if_not(a)` | flags |
| `indent(e, s)`, `same(s)` | columns, `s` says whether to skip before the check |
| `end(s)` | the end of the input, after the skip if `s` |
| `def(x, e, s)`, `ref(x)` | captures |
| `label(e, l, s)` | match `e`. If it fails, stop the parse with the labeled error `l`, or recover from it in recovering mode |
| `error_alternative(e, m)` | fail, and where a failed parse is explained, give the error `m` over what `e` matches where the parse tried it |

## Modules

A grammar with `%extends "m"` is joined with the grammar `m` before anything else, into one grammar that the rest of this page reads:

- `m` is joined with what it extends first.
- Each rule of the grammar whose name `m` defines takes the place of `m`'s rule, and every reference means it. A `...` that stands as a whole alternative of it is replaced by the alternatives of `m`'s rule, in their order.
- The other rules are added.
- The skip expression is the ordered choice of `m`'s `%ignore` expressions and then the grammar's. The flags and soft keywords are those of both, and `%word` and `%tabs` the grammar's where it gives them, otherwise `m`'s.
- A `%recover` of the grammar takes the place of `m`'s for the same label, and `m`'s others are added.

A grammar with `%import m (a, b)` then adds rules of `m`, joined with what it extends and imports first:

- `a` and `b` under their names, and every rule they need, directly or through others, under `m.r` for its name `r`, a name no grammar text can write. The levels of a precedence block come under `m.l`, and the aliases `x` of a rule under a private name under `m.x`. References in the rules added follow the new names. A reference to a name that `m` lacks is an error.
- A name keeps the kind of the name it replaces: `m.R` is a terminal as `R` is, and `m._r` makes no node as `_r` makes none.
- The flags of `m` that the rules added set or test join the grammar's. Its other directives do not count.
- All imports of one grammar `m` add one set of rules, and one without names takes every rule of `m` whose name has no dot. A rule under a private name that the grammar has already, through what it extends or another import, is the same rule and is added once.

## Translation into the core

The translation runs in this order.

1. **Parameters.** Every use `r<a1, ..., an>` of a rule with parameters becomes a call of a rule instance whose body is the template with each parameter replaced by its argument. Uses with structurally equal arguments share one instance. All instances carry the name of the template.
2. **Precedence blocks.** A rule `r: %precedence { L1 ... Ln }` becomes one rule per level. The rule of level `Lk` is `choice(A1, ..., Aj, call(Lk+1))`, with the alternatives `A` of that level and the fall-through to the next level, which the last level does not have. A reference to a level name calls that level's rule, from inside the block or from any other rule, since level names belong to the whole grammar. The rule `r` itself becomes `call(L1)`.
3. **Sugar.** `e?` is `rep(e, 0, 1)`, `e*` is `rep(e, 0, ∞)`, `e+` is `rep(e, 1, ∞)`. `e ** s` is `rep(seq(e, rep(seq(s, e), 0, ∞)), 0, 1)` and `e ++ s` is `seq(e, rep(seq(s, e), 0, ∞))`. `e ^ l` is `label(e, l, s)`, where a message forms a label of its own. `()` is `ε`. A sequence `e1 ... ek %error(m) ...` is `error_alternative(seq(e1, ..., ek), m)`, without the parts after `%error(m)`, which never match, and `%error(m)` alone is `error_alternative(ε, m)`.
4. **Modes.** Every expression is translated in the mode of the rule whose body it ends up in: *syntactic* for syntactic rules, *lexical* for lexical rules, *plain* for terminals and the skip expression.
   - In syntactic mode, a literal, class, property, `.` or `ref(x)` becomes `seq(skip, token(k, h, a))` with the atom `a`, a reference to a terminal `T` becomes `seq(skip, token(T, h, call(T)))`, and a reference to a lexical rule becomes `seq(skip, call(R))`.
   - In lexical mode the same holds without the `skip`.
   - In plain mode atoms stay atoms and a reference to a terminal is `call(T)`. References to syntactic or lexical rules are errors.
   - The kind `k` of an anonymous token is the atom as written, normalized: `"+"`, `[0-9]`, `.`. A token is hidden (`h`) if it comes from an atom or a hidden terminal and is not captured by a field. A field captures the atoms and terminal references that are its whole match: reached from the field through choices, repetitions, the items of separated repetitions, labels, flags, `%indent` and `%def`, not through a sequence of several parts, a separator or a lookahead.
   - In a grammar with `%word`, a literal token whose last character is a word character becomes `token(k, h, seq(a, word_end))`.
   - `!.` becomes `end(s)`, the end of the input.
   - `%indent`, `%same`, `%def`, `e ^ l` and `!.` carry `s = true` in syntactic mode and `s = false` otherwise.
5. **Rules.** The body of a terminal marked `@reserved` becomes `unreserved(body)`. In a syntactic or lexical rule `r`, each top-level alternative `A` becomes `node(alias, A)` when it has an alias, and otherwise `node(r, A)`, `transparent(r, A)` for a transparent rule, or `A` alone for an inlined rule or a precedence level.
6. **Fields.** `label=e` becomes `field(f, e)`.
7. **Recoveries.** `%recover l: r` gives the named label `l` the *recovery* `r`, translated in syntactic mode. A label has at most one.

## Matching

The meaning of a core expression `e` is a relation between `e`, a position `i`, a context `κ`, and a result.

- The context `κ` has four parts: the set of flags that are on, the reference column, the captures, which map names to texts, and the *floor*, a position or none. Two contexts are equal when all four parts are, however the captures came about.
- Matching knows besides the mode of the parse, normal or recovering, and whether it runs *sheltered*, inside a lookahead, a token or the skip expression. Neither is part of the context, so a rule that calls itself through a lookahead finds the entry of the table of left recursion made around it, in either mode.
- A result is one of:
  - **success** `(j, items, Γ, c)`: the expression matched the input from `i` to `j`, produced the list `items`, leaves the captures `Γ` for what follows, and `c` says whether it passed a cut that no choice point has consumed yet;
  - **failure** `(c)`: the expression does not match, and `c` says whether a cut was passed before the failure;
  - **error** `(l, p, D)`: the labeled error `l` occurred at position `p`, after the error items `D` that the matches it ended had produced before it. An error is never caught by a choice and ends the whole parse.

Unless a rule below says otherwise, an error of a part is the error of the whole, with the error items among what the parts before it produced, at any depth, in front of its `D`, and the captures of a success are those of the context.

| Expression | Result |
|---|---|
| `ε` | success `(i, [], Γ, false)` |
| `lit(s)` | success at `i + len(s)` if the input continues with `s` at `i`, otherwise failure. `lit_i` compares ASCII letters without case |
| `class(C)` | success after the character at `i` if it is in `C`, failure at the end of the input or otherwise |
| `any` | success after the character at `i`, failure at the end of the input |
| `seq(e1, ..., en)` | match `e1` at `i`, then each next part where the previous ended, with the captures the previous left. Success if all succeed, with the concatenated items and `c` true if any part passed a cut. Failure at the first failing part, with `c` true if that part or an earlier one passed a cut |
| `choice(e1, ..., en)` | match the alternatives at `i` in order. The first success is the result, with `c` false. A failure with `c` true ends the choice: the result is failure `(false)` and later alternatives are not tried. If all fail, failure `(false)` |
| `rep(e, n, m)` | match `e` repeatedly, each time where the previous ended and with the captures it left, until it fails or `m` matches have been made. A failure of `e` with `c` true makes the result failure `(false)`. A match that consumes no input is the last one: it counts, contributes its items, and the repetition succeeds even if fewer than `n` matches were made, since more would match the same way. Otherwise success if at least `n` matches were made, with their items, and failure `(false)` if not |
| `not(e)` | match `e` at `i`, sheltered: success `(i, [], Γ, false)` if it fails or ends in an error, failure `(false)` if it succeeds |
| `and(e)` | match `e` at `i`, sheltered: success `(i, [], Γ, false)` if it succeeds, failure `(false)` if it fails or ends in an error |
| `cut` | success `(i, [], Γ, true)` |
| `label(e, l, s)` | the result of `e` if it succeeds or ends in an error. If `e` fails, with `c` true or not, let `p` be where `skip` ends at `i` if `s`, otherwise `i`. The result is the error `(l, p, [])`, unless the parse is in recovering mode, the match is not sheltered, the floor is none or lies before `p`, and the grammar gives `l` a recovery `r`. Then `r` is matched at `p` with the floor `p`. On a success at `q`, the result is success `(q, [x], Γ, c)`, with the captures of the context, the cut flag of the failure, and the error item `x` of `l` from `p` to `q` with the items of `r`. If `r` fails, the result is the error `(l, p, [])`, and if it ends in an error, that error. A label is no choice point, so a cut that `e` passes reaches the enclosing one |
| `error_alternative(e, m)` | failure `(false)`. Outside `not` and `and`, the match is an *attempt* of the alternative at `i` in the context `κ` |

The captures do not change inside a `not` or `and`: what `e` captures there is dropped.

### Rules and left recursion

A call `call(R)` at position `i` in context `κ` starts a rule activation. The activation sees the captures of `κ`, and when it ends the caller continues with its own captures. A cut passed inside the activation does not reach the caller.

Rules may be left recursive, directly or through other rules. A table `L` holds, for some triples of rule, position and context, either failure or a success. The table belongs to the parse and starts empty.

1. If `L` has an entry for `(R, i, κ)`, the call returns that entry. This is a left-recursive use.
2. Otherwise `L` gets the entry failure for `(R, i, κ)`, and the body of `R` is matched at `i`.
   - If it fails, the entry is removed and the call fails.
   - If it succeeds ending at `j`, the entry becomes that success and the body is matched again. As long as each new match succeeds and ends further than the entry, the entry becomes the new match and the body is matched again. When a match fails, or does not end further than the entry, the entry is removed and the call returns the entry.
   - An error during these matches removes the entry and is the result of the call.

A rule that is not left recursive gives the same result as matching its body once. A rule that is left and right recursive at once groups to the right.

## Skipping and tokens

- `skip` matches the skip expression at `i`, in plain mode, sheltered, with the current context, then again where it ended, as long as it succeeds and consumes input. Its result is success at the last end, without items, and with the captures of the context: what the skip expression captures holds for that one match. A grammar without `%ignore` has an empty skip expression, so `skip` matches the empty string.
- `token(k, h, e)` matches `e`, sheltered, drops what `e` produces, and on success produces one token of kind `k` from `i` to the end of the match, hidden if `h`.
- `word_end` succeeds without consuming input unless a word character follows at `i`. The word characters are those of `%word`, and `[A-Za-z0-9_]` for reserved words in a grammar without `%word`.
- `unreserved(e)` matches `e` and fails when the matched text is a reserved word. The reserved words are the texts of literals that occur as tokens of syntactic or lexical rules and consist only of word characters, minus the soft keywords. A case-insensitive literal reserves every ASCII spelling of its text.

Where what an expression produces is dropped, in a token, the skip expression and a lookahead, it runs sheltered and nothing recovers, so no recovery goes unreported.

## Context

| Expression | Result |
|---|---|
| `on(a, e)`, `off(a, e)` | the result of `e` with flag `a` added to, or removed from, the flags of the context |
| `if(a)`, `if_not(a)` | success `(i, [], Γ, false)` if flag `a` is on, or off, otherwise failure `(false)` |
| `indent(e, s)` | let `p` be where `skip` ends at `i` if `s`, otherwise `i`. If the column of `p` is greater than the reference column, the result of `e` at `i` with the column of `p` as the reference column, otherwise failure `(false)` |
| `same(s)` | with `p` as above: success `(i, [], Γ, false)` if the column of `p` equals the reference column, otherwise failure `(false)` |
| `end(s)` | with `p` as above: success `(i, [], Γ, false)` if `p` is the end of the input, otherwise failure `(false)` |
| `def(x, e, s)` | with `p` as above: the result of `e`. On success, the captures it leaves map `x` to the text from `p` to the end of the match, in place of any text before, and at most the end of the match |
| `ref(x)` | success after the text `x` maps to if the input continues with it at `i`. Failure if it does not, or if `x` maps to nothing |

Where `indent`, `same`, `end` and `def` take `p` from where `skip` ends, before anything else, an error that `skip` ends in is their result.

Flags and the reference column hold for the extent of their expression, including the rules it calls. Captures hold until the rule activation that made them ends.

## Items and the tree

Matching produces a list of items. An item is a token, with a kind, a position range and a hidden mark, or a node, with a kind and a list of items as children. Both may carry a field.

- `node(k, e)` turns the items of a success of `e` into one node of kind `k`.
- `transparent(k, e)` does the same unless the items contain exactly one item that is not a hidden token. In that case the items are the result as they are.
- `field(f, e)` gives field `f` to each item of a success of `e` that has no field yet and is not a hidden token. Hidden tokens never carry a field.
- The **span** of a node runs from the start of its first token to the end of its last token, at any depth, hidden tokens included. A node without tokens has the empty span at the position where its `node` or `transparent` expression was matched.
- The **error item** of a label `l` from `p` to `q`, with the items `X` of its recovery, is a node of kind `<error>` with the children `X` and the span from `p` to `q`, or, where `q` is `p`, a token of kind `<missing>` with the empty span at `p`, which is never hidden. It records `l`. Where the failed expression is `field(f, e)`, it carries the field `f`. Within the span of its parent it counts as a token.

## Parsing an input

A parse runs in *normal mode* or in *recovering mode*. To parse an input with a start rule `S`, the expression for a reference to `S` in syntactic mode is matched at position 0 in the initial context: no flags, reference column 0, no captures and no floor. On success at `j`, `skip` is matched at `j`, and the parse succeeds if that reaches the end of the input. An error of that `skip` comes after the items of the success, whose error items come first in its `D`. The tree consists of the items of the success. Its *recoveries* are its error items in the order of the text, each with its label and span. A parse in normal mode has none.

A parse that does not succeed fails at the *farthest failure position*, with an *expected set*:

- A `token` expression that fails at position `p` records `p` with the floor of its context, outside of `not`, `and` and the skip expression. Of the positions recorded with no floor, outside every recovery, the largest is the farthest failure position. The expected set holds the kinds of the tokens that failed there, by display name where the grammar gives one.
- An `indent` or `same` whose column check fails records the position it checked in the same way, expecting indentation, and an `end` that fails records it expecting the end of the input.
- A labeled error at position `p` has the expected set of `p` under the floor its label failed with: what failed there in the whole parse with that floor. So what a recovery tries, with a floor of its own, explains no error outside it. A named label shows the set after its name.
- An error item of a label `l` at `p` is explained as the error `l` would have ended the parse with where it first failed at `p` under the floor it failed with: at the farthest position at or after `p` where a failure had been recorded under that floor by then, with what had failed there by then, and at `p` with an empty set where none had. So the first error of a parse in recovering mode is the error a parse in normal mode fails with. A later failure of `l` at `p` under that floor changes nothing, since what fails inside a remembered match is recorded only once.
- If the input does not end where the parse ended, that position counts as a failure that expects the end of the input. The recoveries of the failed parse are the error items of the success.
- A parse that ends in an error `(l, p, D)` fails with that error instead, and its recoveries are the error items in `D`. Where the label failed outside every recovery and a failure outside every recovery was recorded beyond `p`, the parse got further than the label, and it fails at the farthest failure position with its expected set instead, as a parse that does not match, keeping those recoveries.

A parse that fails, other than at a limit, is *explained* where the grammar has an error alternative. Its attempts under the floor of its error, which is that of the label and none for a parse that does not match, are tested in the order of their positions, and at one position in the order of the alternatives in the grammar: `e` is matched at `i` in `κ`, sheltered, as a parse of its own. The first alternative whose `e` succeeds gives the error `m` from `i` to `j`, the farthest end where it succeeds in several contexts, in place of the error, with no expected set, and the parse keeps its recoveries. Since an attempt is tested on its own, what the parse remembered and the order it tried things in cannot change which one explains it. Explaining is part of the parse, so a limit that the tests reach ends the parse at that limit.

An input that a parse in normal mode accepts fails no label outside a token, the skip expression and a lookahead, so a parse in recovering mode gives it the same tree and no recoveries.

## Limits

An implementation stops a parse that exceeds a limit on time, recursion depth, memory or input size, and reports which limit it hit. A limit never changes the result of a parse that stays within it.

Recovery needs no limit of its own. A recovery that matches nothing is an empty match, which ends a repetition and takes part in left recursion like any other, and a recovery inside another starts past where the other started.
