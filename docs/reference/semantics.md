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
| `def(x, e, s)`, `ref(x)` | captures |
| `label(e, l, s)` | match `e`, and stop the parse with the labeled error `l` if it fails |
| `error_alternative(m)` | fail. Reserved for explaining errors |

## Translation into the core

The translation runs in this order.

1. **Parameters.** Every use `r<a1, ..., an>` of a rule with parameters becomes a call of a rule instance whose body is the template with each parameter replaced by its argument. Uses with structurally equal arguments share one instance. All instances carry the name of the template.
2. **Precedence blocks.** A rule `r: %precedence { L1 ... Ln }` becomes one rule per level. The rule of level `Lk` is `choice(A1, ..., Aj, call(Lk+1))`, with the alternatives `A` of that level and the fall-through to the next level, which the last level does not have. A reference to a level name calls that level's rule, from inside the block or from any other rule, since level names belong to the whole grammar. The rule `r` itself becomes `call(L1)`.
3. **Sugar.** `e?` is `rep(e, 0, 1)`, `e*` is `rep(e, 0, ∞)`, `e+` is `rep(e, 1, ∞)`. `e ** s` is `rep(seq(e, rep(seq(s, e), 0, ∞)), 0, 1)` and `e ++ s` is `seq(e, rep(seq(s, e), 0, ∞))`. `e ^ l` is `label(e, l, s)`, where a message forms a label of its own. `()` is `ε`.
4. **Modes.** Every expression is translated in the mode of the rule whose body it ends up in: *syntactic* for syntactic rules, *lexical* for lexical rules, *plain* for terminals and the skip expression.
   - In syntactic mode, a literal, class, property, `.` or `ref(x)` becomes `seq(skip, token(k, h, a))` with the atom `a`, a reference to a terminal `T` becomes `seq(skip, token(T, h, call(T)))`, and a reference to a lexical rule becomes `seq(skip, call(R))`.
   - In lexical mode the same holds without the `skip`.
   - In plain mode atoms stay atoms and a reference to a terminal is `call(T)`. References to syntactic or lexical rules are errors.
   - The kind `k` of an anonymous token is the atom as written, normalized: `"+"`, `[0-9]`, `.`. A token is hidden (`h`) if it comes from an atom or a hidden terminal and is not captured by a field. A field captures the atoms and terminal references that are its whole match: reached from the field through choices, repetitions, the items of separated repetitions, labels, flags, `%indent` and `%def`, not through a sequence of several parts, a separator or a lookahead.
   - In a grammar with `%word`, a literal token whose last character is a word character becomes `token(k, h, seq(a, word_end))`.
   - `%indent`, `%same`, `%def` and `e ^ l` carry `s = true` in syntactic mode and `s = false` otherwise.
5. **Rules.** The body of a terminal marked `@reserved` becomes `unreserved(body)`. In a syntactic or lexical rule `r`, each top-level alternative `A` becomes `node(alias, A)` when it has an alias, and otherwise `node(r, A)`, `transparent(r, A)` for a transparent rule, or `A` alone for an inlined rule or a precedence level.
6. **Fields.** `label=e` becomes `field(f, e)`.

## Matching

The meaning of a core expression `e` is a relation between `e`, a position `i`, a context `κ`, and a result.

- The context `κ` has three parts: the set of flags that are on, the reference column, and the captures, which map names to texts. Two contexts are equal when all three parts are, however the captures came about.
- A result is one of:
  - **success** `(j, items, Γ, c)`: the expression matched the input from `i` to `j`, produced the list `items`, leaves the captures `Γ` for what follows, and `c` says whether it passed a cut that no choice point has consumed yet;
  - **failure** `(c)`: the expression does not match, and `c` says whether a cut was passed before the failure;
  - **error** `(l, p)`: the labeled error `l` occurred at position `p`. An error is never caught by a choice and ends the whole parse.

Unless a rule below says otherwise, an error result of a part is the error result of the whole, and the captures of a success are those of the context.

| Expression | Result |
|---|---|
| `ε` | success `(i, [], Γ, false)` |
| `lit(s)` | success at `i + len(s)` if the input continues with `s` at `i`, otherwise failure. `lit_i` compares ASCII letters without case |
| `class(C)` | success after the character at `i` if it is in `C`, failure at the end of the input or otherwise |
| `any` | success after the character at `i`, failure at the end of the input |
| `seq(e1, ..., en)` | match `e1` at `i`, then each next part where the previous ended, with the captures the previous left. Success if all succeed, with the concatenated items and `c` true if any part passed a cut. Failure at the first failing part, with `c` true if that part or an earlier one passed a cut |
| `choice(e1, ..., en)` | match the alternatives at `i` in order. The first success is the result, with `c` false. A failure with `c` true ends the choice: the result is failure `(false)` and later alternatives are not tried. If all fail, failure `(false)` |
| `rep(e, n, m)` | match `e` repeatedly, each time where the previous ended and with the captures it left, until it fails or `m` matches have been made. A failure of `e` with `c` true makes the result failure `(false)`. A match that consumes no input is the last one: it counts, contributes its items, and the repetition succeeds even if fewer than `n` matches were made, since more would match the same way. Otherwise success if at least `n` matches were made, with their items, and failure `(false)` if not |
| `not(e)` | match `e` at `i`: success `(i, [], Γ, false)` if it fails or ends in an error, failure `(false)` if it succeeds |
| `and(e)` | success `(i, [], Γ, false)` if `e` succeeds, failure `(false)` if it fails or ends in an error |
| `cut` | success `(i, [], Γ, true)` |
| `label(e, l, s)` | the result of `e` if it succeeds or ends in an error. If `e` fails, with `c` true or not, the error `(l, p)`, where `p` is where `skip` ends at `i` if `s`, otherwise `i`. A label is no choice point, so a cut that `e` passes reaches the enclosing one |
| `error_alternative(m)` | failure `(false)` |

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

- `skip` matches the skip expression at `i`, in plain mode, with the current context, then again where it ended, as long as it succeeds and consumes input. Its result is success at the last end, without items, and with the captures of the context: what the skip expression captures holds for that one match. A grammar without `%ignore` has an empty skip expression, so `skip` matches the empty string.
- `token(k, h, e)` matches `e`, drops what `e` produces, and on success produces one token of kind `k` from `i` to the end of the match, hidden if `h`.
- `word_end` succeeds without consuming input unless a word character follows at `i`. The word characters are those of `%word`, and `[A-Za-z0-9_]` for reserved words in a grammar without `%word`.
- `unreserved(e)` matches `e` and fails when the matched text is a reserved word. The reserved words are the texts of literals that occur as tokens of syntactic or lexical rules and consist only of word characters, minus the soft keywords. A case-insensitive literal reserves every ASCII spelling of its text.

## Context

| Expression | Result |
|---|---|
| `on(a, e)`, `off(a, e)` | the result of `e` with flag `a` added to, or removed from, the flags of the context |
| `if(a)`, `if_not(a)` | success `(i, [], Γ, false)` if flag `a` is on, or off, otherwise failure `(false)` |
| `indent(e, s)` | let `p` be where `skip` ends at `i` if `s`, otherwise `i`. If the column of `p` is greater than the reference column, the result of `e` at `i` with the column of `p` as the reference column, otherwise failure `(false)` |
| `same(s)` | with `p` as above: success `(i, [], Γ, false)` if the column of `p` equals the reference column, otherwise failure `(false)` |
| `def(x, e, s)` | the result of `e`. On success, the captures it leaves map `x` to the text from `p` to the end of the match, in place of any text before, where `p` is where `skip` ends at `i` if `s`, otherwise `i`, and at most the end of the match |
| `ref(x)` | success after the text `x` maps to if the input continues with it at `i`. Failure if it does not, or if `x` maps to nothing |

Flags and the reference column hold for the extent of their expression, including the rules it calls. Captures hold until the rule activation that made them ends.

## Items and the tree

Matching produces a list of items. An item is a token, with a kind, a position range and a hidden mark, or a node, with a kind and a list of items as children. Both may carry a field.

- `node(k, e)` turns the items of a success of `e` into one node of kind `k`.
- `transparent(k, e)` does the same unless the items contain exactly one item that is not a hidden token. In that case the items are the result as they are.
- `field(f, e)` gives field `f` to each item of a success of `e` that has no field yet and is not a hidden token. Hidden tokens never carry a field.
- The **span** of a node runs from the start of its first token to the end of its last token, at any depth, hidden tokens included. A node without tokens has the empty span at the position where its `node` or `transparent` expression was matched.

## Parsing an input

To parse an input with a start rule `S`, the expression for a reference to `S` in syntactic mode is matched at position 0 in the initial context: no flags, reference column 0, no captures. On success at `j`, `skip` is matched at `j`, and the parse succeeds if that reaches the end of the input. The tree consists of the items of the success.

A parse that does not succeed fails at the *farthest failure position*, with an *expected set*:

- A `token` expression that fails at position `p` records `p`, outside of `not`, `and` and the skip expression. Of all recorded positions the largest is the farthest failure position. The expected set holds the kinds of the tokens that failed there, by display name where the grammar gives one.
- An `indent` or `same` whose column check fails records the position it checked, expecting indentation.
- If the input does not end where the parse ended, that position counts as a failure that expects the end of the input.
- A parse that ends in an error fails with that error instead.

## Limits

An implementation stops a parse that exceeds a limit on time, recursion depth, memory or input size, and reports which limit it hit. A limit never changes the result of a parse that stays within it.
