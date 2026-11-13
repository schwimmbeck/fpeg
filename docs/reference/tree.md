# Parse tree

A successful parse produces one tree of nodes and tokens. The tree is lossless: every character the parse matched lies in a token, and only the text the `%ignore` expression skipped lies between tokens.

## Nodes and tokens

```fpeg
start: pair ("," pair)*
pair: key=NAME "=" value=NUMBER
NAME: [a-z]+
NUMBER: [0-9]+
%ignore " "+
```

For the input `x = 1, y = 2` this grammar gives the tree that `fastpeg parse` prints as

```text
start
  pair
    key=NAME"x"
    ~"="
    value=NUMBER"1"
  ~","
  pair
    key=NAME"y"
    ~"="
    value=NUMBER"2"
```

- A **node** comes from a syntactic or lexical rule. Its kind is the rule's name, or the alias `-> name` of the alternative that matched, and its children are what the rule's body produced.
- A **token** comes from a terminal used in a syntactic or lexical rule, or from a literal, class, property, `.` or `%ref` written in one. A terminal's kind is its name. An anonymous token's kind is the literal, class or property as written, such as `"="`.
- A **hidden** token is kept in the tree but skipped by views: anonymous tokens and hidden terminals `_NAME` are hidden, unless a field captures them.
- A **field** gives a visible item a name in its node, such as `key` and `value` above. Hidden tokens never carry a field.
- Rules that are inlined `_name`, and transparent rules `?name` with one visible child, leave no node of their own: their children join their parent's.

## Spans

- Every node and token has a span: the byte offsets in the UTF-8 input where it starts and where it ends, the end not included.
- A node spans from the start of its first token to the end of its last token, at any depth and hidden tokens included. Skipped text never widens a span, so the first `pair` above spans `x = 1`, not the space after it.
- A node without tokens has an empty span where its rule started matching.
- Lines and columns are worked out from the offsets when they are needed, so the tree holds one unit only.

## Layout

The tree is one compact array in postorder: the children of a node come before the node, and each entry holds its kind, whether it is a token and whether it is hidden, its span, the size of its subtree and its field. A pass that builds values bottom-up is one scan from the start, and a node's subtree is the entries that end at the node.
