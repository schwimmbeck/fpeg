// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Facts about a resolved grammar: which rules are left recursive, found with which expressions
//! can succeed without consuming input, and what each expression can start with. All take
//! time linear in the size of the grammar, over a graph with one node per expression, a shared
//! expression once, and an edge from every call to the body of the rule it calls.

use std::collections::{HashMap, HashSet};

use crate::grammar::ClassItem;
use crate::ir::{self, RuleId};
use crate::tree::KindId;

/// The `need` of a node that never succeeds without consuming input.
const NEVER: u32 = u32::MAX;

/// The most kinds of head tokens kept for an expression. One with more gets no test that
/// records them.
const MAX_HEAD_KINDS: usize = 64;

/// How the children of a node are matched.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    /// In turn, each where the one before ended.
    Sequence,
    /// Each at the start of the node, or not at all.
    Start,
}

/// What a node does before it consumes its first byte, for [`Starts`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    /// Consumes a first byte from the set: a literal, a class, any character.
    Bytes(Bytes),
    /// Consumes nothing and tests nothing: the empty expression, a repetition of no steps.
    Empty,
    /// The skip, after which the parts that follow it look at the next byte.
    Skip,
    /// Consumes nothing but may fail: a lookahead, a flag test, the end of a word.
    Test,
    /// A cut, which commits a choice.
    Cut,
    /// Column checks, captures and the alternative for errors: a column check can fail before
    /// its first byte, depending on the column and not on the byte, which no test can tell.
    Unknown,
    /// The children in turn.
    Sequence,
    /// Any of the children: a choice, a repetition, and the forms that wrap one child.
    Alternatives,
    /// A call of the rule, whose body is its child.
    Call(RuleId),
    /// A token of the kind around its child.
    Token(KindId),
    /// A label around its child, which throws when its child fails.
    Label,
    /// A flag turned on or off for the child.
    Flags,
}

struct Node {
    form: Form,
    order: Order,
    /// How many of its children must succeed without consuming input for the node to, or
    /// [`NEVER`].
    need: u32,
    /// The children, a range of [`Graph::edges`].
    first: u32,
    len: u32,
}

/// The expressions of a grammar as a graph. See the module documentation.
struct Graph {
    nodes: Vec<Node>,
    edges: Vec<u32>,
    /// The node of each rule's body.
    bodies: Vec<u32>,
    /// Calls to link once every body has its node: the node of the call and the rule called.
    calls: Vec<(u32, RuleId)>,
    /// The node of each shared expression, which every use links to.
    shared: HashMap<*const ir::Expr, u32>,
    /// The node of every expression.
    node_of: HashMap<*const ir::Expr, u32>,
    /// Whether each label has a recovery, by label id.
    recovered: Vec<bool>,
    /// Edges to link to the node of a label's recovery once every recovery has one.
    label_links: Vec<(u32, ir::LabelId)>,
}

/// What the compiler needs to know about the rules of a grammar, indexed by rule.
pub(crate) struct Facts {
    /// Whether the rule can call itself where it started, through calls that consume nothing
    /// before them.
    pub left_recursive: Vec<bool>,
    /// Whether the rule can call itself anywhere in its body.
    pub recursive: Vec<bool>,
    graph: Graph,
    nullable: Vec<bool>,
    /// Which nodes can succeed without consuming input past a skip, see [`Start::skips`].
    skips: Vec<bool>,
    /// The cycle of left children each node lies on, as the number of one of its nodes.
    left_component: Vec<u32>,
    /// The nodes, each after its left children, but for those on a cycle with it, so that a
    /// fact that passes from child to parent passes along an edge once in this order.
    left_order: Vec<u32>,
}

/// A set of bytes, one bit each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Bytes(pub [u64; 4]);

impl Bytes {
    pub const ALL: Bytes = Bytes([u64::MAX; 4]);

    pub fn add(&mut self, byte: u8) {
        self.0[usize::from(byte >> 6)] |= 1 << (byte & 63);
    }

    pub fn contains(&self, byte: u8) -> bool {
        self.0[usize::from(byte >> 6)] >> (byte & 63) & 1 == 1
    }

    pub fn union(self, other: Bytes) -> Bytes {
        Bytes([0, 1, 2, 3].map(|i| self.0[i] | other.0[i]))
    }

    /// The first bytes of the code points of a class. Beyond ASCII every byte is taken, unless
    /// the class holds no code point there.
    fn of_class(set: &ir::CharSet) -> Bytes {
        let mut bytes = Bytes::default();
        for byte in 0..0x80u8 {
            if set.contains(char::from(byte)) {
                bytes.add(byte);
            }
        }
        let only_ascii = set.items.iter().all(|item| match *item {
            ClassItem::Range(_, last) => last.is_ascii(),
            ClassItem::Property { .. } => false,
        });
        if set.negated || !only_ascii {
            bytes.0[2] = u64::MAX;
            bytes.0[3] = u64::MAX;
        }
        bytes
    }

    /// The first bytes of a literal that is not empty.
    fn of_literal(text: &str, case_insensitive: bool) -> Bytes {
        let mut bytes = Bytes::default();
        if let Some(&byte) = text.as_bytes().first() {
            bytes.add(byte);
            if case_insensitive {
                bytes.add(byte.to_ascii_lowercase());
                bytes.add(byte.to_ascii_uppercase());
            }
        }
        bytes
    }
}

/// What an expression can start with, for testing the next byte before trying it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Start {
    /// The bytes its first consumed byte can be.
    pub bytes: Bytes,
    /// Whether it can succeed without consuming input.
    pub nullable: bool,
    /// Whether it can succeed without consuming input past a skip, which may have moved past
    /// ignored text all the same, so that what follows it may start after a skip.
    pub skips: bool,
    /// Whether some first byte is consumed after a skip that comes before it.
    pub after_skip: bool,
    /// Whether some first byte is consumed with no skip before it.
    pub at_once: bool,
    /// Whether every failure before its first byte is a plain one: no label throws and no cut
    /// commits on the way.
    pub plain: bool,
    /// Whether it can open a token before its first byte, whose failure would be recorded.
    pub tokens: bool,
    /// Whether a failure at its first byte surely tries its head tokens: no lookahead, flag
    /// test or end of a word before them, which can fail without trying them.
    pub clean: bool,
    /// Whether it can call a rule that keeps the growing loop of left recursion before its
    /// first byte, which where its frame started gives its seed without trying a token.
    pub growing: bool,
}

impl Start {
    /// What nothing starts with, where the search for a fixpoint begins.
    const NOTHING: Start = Start {
        bytes: Bytes([0; 4]),
        nullable: false,
        skips: false,
        after_skip: false,
        at_once: false,
        plain: true,
        tokens: false,
        clean: true,
        growing: false,
    };

    /// What an expression whose start is unknown can start with.
    const UNKNOWN: Start = Start {
        bytes: Bytes::ALL,
        nullable: true,
        skips: true,
        after_skip: true,
        at_once: true,
        plain: false,
        tokens: true,
        clean: false,
        growing: true,
    };

    pub fn merge(self, other: Start) -> Start {
        Start {
            bytes: self.bytes.union(other.bytes),
            nullable: self.nullable || other.nullable,
            skips: self.skips || other.skips,
            after_skip: self.after_skip || other.after_skip,
            at_once: self.at_once || other.at_once,
            plain: self.plain && other.plain,
            tokens: self.tokens || other.tokens,
            clean: self.clean && other.clean,
            growing: self.growing || other.growing,
        }
    }
}

/// What every expression of a grammar can start with. See [`Facts::starts`].
pub(crate) struct Starts<'f> {
    node_of: &'f HashMap<*const ir::Expr, u32>,
    values: Vec<Start>,
    /// The kinds of the head tokens of each node, sorted, or none where there are too many.
    heads: Vec<Option<Vec<KindId>>>,
}

impl Starts<'_> {
    /// What `expr`, an expression of the grammar, can start with. Anything else may start
    /// with anything.
    pub fn of(&self, expr: &ir::Expr) -> Start {
        (self.node_of.get(&std::ptr::from_ref(expr)))
            .and_then(|&node| self.values.get(node as usize))
            .copied()
            .unwrap_or(Start::UNKNOWN)
    }

    /// The kinds of the tokens that `expr` tries at its start and that fail there, where it
    /// cannot start with the next byte and a failure at its first byte is clean: those of
    /// the parts of a sequence up to the first that cannot succeed without consuming input,
    /// and of the alternatives of a choice up to the first that can, which then succeeds.
    /// A token that can match nothing never fails. None where there are too many to keep.
    pub fn heads(&self, expr: &ir::Expr) -> Option<&[KindId]> {
        let node = self.node_of.get(&std::ptr::from_ref(expr))?;
        self.heads.get(*node as usize)?.as_deref()
    }

    /// What the parts, matched in turn, start with, and the kinds of their head tokens, as
    /// for a sequence of them, with the roles of [`Graph::roles`].
    pub fn sequence(&self, parts: &[ir::Expr]) -> (Start, Option<Vec<KindId>>) {
        let mut start = Start::NOTHING;
        let mut heads = Some(Vec::new());
        let mut role = Role::Join;
        let mut skipped = false;
        start.nullable = true;
        for part in parts {
            if matches!(part, ir::Expr::Skip) {
                role = Role::AfterSkip;
                skipped = true;
                continue;
            }
            let found = self.of(part);
            start = start.merge(Start {
                nullable: true,
                skips: false,
                ..role.added(found, false)
            });
            heads = joined(heads, self.heads(part));
            if !found.nullable {
                start.nullable = false;
                break;
            }
            if found.skips {
                skipped = true;
                role = role.past_skip();
            }
        }
        start.skips = start.nullable && skipped;
        (start, heads)
    }
}

/// Find the facts of a grammar.
pub(crate) fn analyze(grammar: &ir::Grammar) -> Facts {
    let mut graph = Graph {
        nodes: Vec::new(),
        edges: Vec::new(),
        bodies: Vec::new(),
        calls: Vec::new(),
        shared: HashMap::new(),
        node_of: HashMap::new(),
        recovered: grammar.recoveries.iter().map(Option::is_some).collect(),
        label_links: Vec::new(),
    };
    for rule in &grammar.rules {
        let body = graph.node(&rule.body);
        graph.bodies.push(body);
    }
    // The skip expression and the entries are compiled too.
    for expr in grammar.skip.iter().chain(grammar.entry.values()) {
        graph.node(expr);
    }
    let recoveries: Vec<Option<u32>> = (grammar.recoveries.iter())
        .map(|recovery| recovery.as_ref().map(|expr| graph.node(expr)))
        .collect();
    for (edge, label) in std::mem::take(&mut graph.label_links) {
        if let Some(&Some(node)) = recoveries.get(label.0 as usize) {
            graph.edges[edge as usize] = node;
        }
    }
    for (node, rule) in std::mem::take(&mut graph.calls) {
        let call = &mut graph.nodes[node as usize];
        match graph.bodies.get(rule.0 as usize) {
            Some(&body) => graph.edges[call.first as usize] = body,
            // A call to a rule that does not exist fails, as the reference interpreter has it.
            None => {
                call.need = NEVER;
                call.len = 0;
            }
        }
    }
    let parents = graph.parents();
    let nullable = graph.nullable(&parents);
    let skips = graph.skips(&parents, &nullable);
    drop(parents);
    let (left, left_component, left_order) = graph.cyclic(&graph.left_counts(&nullable));
    let all: Vec<u32> = graph.nodes.iter().map(|node| node.len).collect();
    let (anywhere, _, _) = graph.cyclic(&all);
    Facts {
        left_recursive: graph.bodies.iter().map(|&b| left[b as usize]).collect(),
        recursive: graph.bodies.iter().map(|&b| anywhere[b as usize]).collect(),
        graph,
        nullable,
        skips,
        left_component,
        left_order,
    }
}

impl Facts {
    /// The rules that a choice point can call twice at one position: through the starts of
    /// two of its alternatives, or of a part that can end after a failed try, as `e?` and
    /// `e+` can, and what follows it. Only the calls an expression makes itself count, not
    /// those of the rules it calls, since remembering a rule saves the calls it makes in
    /// turn. Calls inside a token, of terminals, take about what remembering them would, a
    /// rule's calls of itself at its start are its left recursion, which its seed answers,
    /// and a lookahead calls quietly, which the memo keeps apart from other calls.
    pub fn tried_twice(&self, grammar: &ir::Grammar) -> Vec<bool> {
        let mut walk = Twice {
            facts: self,
            twice: vec![false; grammar.rules.len()],
            shared: HashMap::new(),
            rule: RuleId(0),
            sets: vec![0; grammar.rules.len()],
            newest: 0,
        };
        for (i, rule) in grammar.rules.iter().enumerate() {
            walk.rule = RuleId(i as u32);
            walk.calls(&rule.body);
        }
        walk.twice
    }

    /// Whether an expression of the grammar can succeed without consuming input.
    fn nullable_expr(&self, expr: &ir::Expr) -> bool {
        let node = self.graph.node_of.get(&std::ptr::from_ref(expr));
        node.is_none_or(|&node| self.nullable[node as usize])
    }

    /// Whether the rule can succeed without consuming input.
    pub fn rule_nullable(&self, rule: RuleId) -> bool {
        let body = self.graph.bodies.get(rule.0 as usize);
        body.is_some_and(|&body| self.nullable[body as usize])
    }

    /// Whether `expr`, which the body of a rule on a left cycle starts with, can call the
    /// rule before it consumes input. The rule's body starts with `expr`, so `expr` can
    /// exactly where it lies on a cycle of left children with the body.
    pub fn reaches_at_start(&self, expr: &ir::Expr, rule: RuleId) -> bool {
        let node = self.graph.node_of.get(&std::ptr::from_ref(expr));
        let body = self.graph.bodies.get(rule.0 as usize);
        match (node, body) {
            (Some(&node), Some(&body)) => {
                self.left_component[node as usize] == self.left_component[body as usize]
            }
            _ => true,
        }
    }

    /// What every expression can start with. A node's start is what it adds itself, joined
    /// with what each child it starts with adds through its role. Starts only grow, each a
    /// bounded number of times, by a byte or a fact, and each growth is passed once along
    /// every edge to a parent, so the time stays linear. Children come first, so that outside
    /// a cycle a node passes its start on once, when it is complete.
    ///
    /// Where the skip expression tests a flag, a skip under a flag turned on or off may end
    /// elsewhere than a skip before the flag changed, so the expression under the flag, and
    /// what follows a skip in it, may start with any byte.
    ///
    /// `growing` says which rules keep the growing loop.
    pub fn starts(&self, grammar: &ir::Grammar, growing: &[bool]) -> Starts<'_> {
        let graph = &self.graph;
        let skip_reads_flags = (grammar.skip.as_ref())
            .is_some_and(|s| reaches(grammar, s, |e| matches!(e, ir::Expr::FlagTest { .. })));
        let mut parents: Vec<Vec<(u32, Role)>> = vec![Vec::new(); graph.nodes.len()];
        for node in 0..graph.nodes.len() as u32 {
            for (child, role) in graph.roles(node, &self.nullable, &self.skips) {
                parents[child as usize].push((node, role));
            }
        }
        let own = |node: &Node| node.own_start(growing);
        let mut values: Vec<Start> = graph.nodes.iter().map(own).collect();
        let mut queued = vec![true; graph.nodes.len()];
        let mut todo: Vec<u32> = self.left_order.iter().rev().copied().collect();
        while let Some(child) = todo.pop() {
            queued[child as usize] = false;
            let start = values[child as usize];
            let moved = skip_reads_flags && (start.after_skip || self.skips[child as usize]);
            for &(parent, role) in &parents[child as usize] {
                let joined = values[parent as usize].merge(role.added(start, moved));
                if joined != values[parent as usize] {
                    values[parent as usize] = joined;
                    if !std::mem::replace(&mut queued[parent as usize], true) {
                        todo.push(parent);
                    }
                }
            }
        }
        for ((start, &nullable), &skips) in values.iter_mut().zip(&self.nullable).zip(&self.skips) {
            start.nullable = nullable;
            start.skips = skips;
        }
        Starts {
            node_of: &graph.node_of,
            values,
            heads: self.heads(),
        }
    }

    /// The kinds of the head tokens of every node, found as the starts are, children first:
    /// each node adds its own, a token the kind of its own unless it can match nothing, and
    /// joins those of the children it tries at its start before it fails there.
    fn heads(&self) -> Vec<Option<Vec<KindId>>> {
        let graph = &self.graph;
        let mut parents: Vec<Vec<u32>> = vec![Vec::new(); graph.nodes.len()];
        for node in 0..graph.nodes.len() as u32 {
            for child in graph.head_children(node, &self.nullable) {
                parents[child as usize].push(node);
            }
        }
        let mut heads: Vec<Option<Vec<KindId>>> = (0..graph.nodes.len() as u32)
            .map(|node| match graph.nodes[node as usize].form {
                Form::Token(kind) => {
                    let child = graph.children(node).first();
                    let fails = child.is_some_and(|&c| !self.nullable[c as usize]);
                    Some(if fails { vec![kind] } else { Vec::new() })
                }
                Form::Unknown => None,
                _ => Some(Vec::new()),
            })
            .collect();
        let mut queued = vec![true; graph.nodes.len()];
        let mut todo: Vec<u32> = self.left_order.iter().rev().copied().collect();
        while let Some(child) = todo.pop() {
            queued[child as usize] = false;
            for &parent in &parents[child as usize] {
                let added = heads[child as usize].as_deref();
                let grown = joined(heads[parent as usize].clone(), added);
                if grown != heads[parent as usize] {
                    heads[parent as usize] = grown;
                    if !std::mem::replace(&mut queued[parent as usize], true) {
                        todo.push(parent);
                    }
                }
            }
        }
        heads
    }
}

/// How the start of a child enters the start of its parent.
#[derive(Clone, Copy)]
enum Role {
    /// As it is.
    Join,
    /// After a skip, so that its first bytes are all consumed after one.
    AfterSkip,
    /// After a part that may have skipped or not, so that its first bytes may be consumed
    /// after a skip or with none.
    MaybeAfterSkip,
    /// Under a flag turned on or off.
    Flags,
    /// Inside a token that cannot match nothing.
    Token,
}

impl Role {
    /// What a child with this start adds to the start of its parent, `moved` where a skip in
    /// the child may end elsewhere than one before it, since the child changes a flag that the
    /// skip expression tests.
    fn added(self, start: Start, moved: bool) -> Start {
        match self {
            Role::Join => start,
            Role::AfterSkip => Start {
                after_skip: start.after_skip || start.at_once,
                at_once: false,
                ..start
            },
            Role::MaybeAfterSkip => Start {
                after_skip: start.after_skip || start.at_once,
                ..start
            },
            Role::Flags if moved => Start {
                bytes: Bytes::ALL,
                ..start
            },
            Role::Flags => start,
            Role::Token => Start {
                clean: true,
                ..start
            },
        }
    }

    /// The role of the parts of a sequence after a part in this role that may end past a skip.
    /// After a skip a second one ends where the first did, since it skips with the same flags.
    fn past_skip(self) -> Role {
        match self {
            Role::Join => Role::MaybeAfterSkip,
            role => role,
        }
    }
}

impl Node {
    /// What a node adds to its start itself, before its children.
    fn own_start(&self, growing: &[bool]) -> Start {
        match self.form {
            Form::Bytes(bytes) => Start {
                bytes,
                at_once: true,
                ..Start::NOTHING
            },
            Form::Cut | Form::Label => Start {
                plain: false,
                clean: false,
                ..Start::NOTHING
            },
            Form::Test => Start {
                clean: false,
                ..Start::NOTHING
            },
            Form::Token(_) => Start {
                tokens: true,
                ..Start::NOTHING
            },
            Form::Call(rule) => Start {
                growing: growing.get(rule.0 as usize).copied().unwrap_or(false),
                ..Start::NOTHING
            },
            Form::Unknown => Start::UNKNOWN,
            Form::Empty | Form::Skip | Form::Sequence | Form::Alternatives | Form::Flags => {
                Start::NOTHING
            }
        }
    }
}

/// The walk of [`Facts::tried_twice`] over the bodies of the rules.
struct Twice<'f> {
    facts: &'f Facts,
    twice: Vec<bool>,
    /// The rules each shared part calls at its start.
    shared: HashMap<*const ir::Expr, Vec<u32>>,
    /// The rule whose body is walked.
    rule: RuleId,
    /// For each rule, the newest set of calls it was put in, so that finding the rules that
    /// two lists share takes time linear in their lengths.
    sets: Vec<u64>,
    /// The number of the newest set.
    newest: u64,
}

impl Twice<'_> {
    fn mark(&mut self, called: u32) {
        if called != self.rule.0
            && let Some(twice) = self.twice.get_mut(called as usize)
        {
            *twice = true;
        }
    }

    /// A new set of calls, which holds none.
    fn new_set(&mut self) -> u64 {
        self.newest += 1;
        self.newest
    }

    /// Mark the rules of `calls` that `set` holds.
    fn mark_held(&mut self, set: u64, calls: &[u32]) {
        for &called in calls {
            if self.sets.get(called as usize) == Some(&set) {
                self.mark(called);
            }
        }
    }

    /// Put the rules of `calls` in `set`, and append those it did not hold to `list`.
    fn put(&mut self, set: u64, calls: &[u32], list: &mut Vec<u32>) {
        for &called in calls {
            if let Some(stamp) = self.sets.get_mut(called as usize)
                && *stamp != set
            {
                *stamp = set;
                list.push(called);
            }
        }
    }

    /// Whether a part of a sequence can end after a try that failed, so that what follows
    /// it starts where the try did.
    fn goes_on_after_failing(&self, part: &ir::Expr) -> bool {
        match part {
            ir::Expr::Repeat { min, max, .. } => max.is_none_or(|max| max > *min),
            _ => self.facts.nullable_expr(part),
        }
    }

    /// The rules `expr` calls at its start, each once, marking those its choice points call
    /// twice. It recurses as deeply as the expression nests, which the resolver bounds.
    fn calls(&mut self, expr: &ir::Expr) -> Vec<u32> {
        use ir::Expr as E;
        match expr {
            E::Call(rule) => vec![rule.0],
            E::Seq(parts) => {
                let starts: Vec<Vec<u32>> = parts.iter().map(|part| self.calls(part)).collect();
                // What the sequence goes on with after each part, from the last part back.
                let mut set = self.new_set();
                let mut after: Vec<u32> = Vec::new();
                for (part, calls) in parts.iter().zip(&starts).rev() {
                    if self.goes_on_after_failing(part) {
                        self.mark_held(set, calls);
                    }
                    if !self.facts.nullable_expr(part) {
                        set = self.new_set();
                        after.clear();
                    }
                    self.put(set, calls, &mut after);
                }
                after
            }
            E::Choice(alternatives) => {
                let starts: Vec<Vec<u32>> = (alternatives.iter())
                    .map(|alternative| self.calls(alternative))
                    .collect();
                let set = self.new_set();
                let mut seen = Vec::new();
                for calls in &starts {
                    self.mark_held(set, calls);
                    self.put(set, calls, &mut seen);
                }
                seen
            }
            E::Repeat { max: Some(0), .. } | E::Token { .. } | E::And(_) | E::Not(_) => Vec::new(),
            E::Repeat { expr, .. }
            | E::Node { expr, .. }
            | E::Transparent { expr, .. }
            | E::Field { expr, .. }
            | E::Unreserved(expr)
            | E::FlagOn { expr, .. }
            | E::FlagOff { expr, .. }
            | E::Indent { expr, .. }
            | E::Def { expr, .. }
            | E::Label { expr, .. } => self.calls(expr),
            E::Shared(inner) => {
                let key = std::sync::Arc::as_ptr(inner);
                if let Some(calls) = self.shared.get(&key) {
                    return calls.clone();
                }
                let calls = self.calls(inner);
                self.shared.insert(key, calls.clone());
                calls
            }
            E::Empty
            | E::Literal { .. }
            | E::Class(_)
            | E::Any
            | E::Cut
            | E::Skip
            | E::WordEnd
            | E::FlagTest { .. }
            | E::Same { .. }
            | E::Ref { .. }
            | E::ErrorAlternative => Vec::new(),
        }
    }
}

/// The kinds of head tokens of both, sorted, or none where there are too many to keep.
pub(crate) fn joined(known: Option<Vec<KindId>>, added: Option<&[KindId]>) -> Option<Vec<KindId>> {
    let (mut known, added) = (known?, added?);
    let new: Vec<KindId> = (added.iter().copied())
        .filter(|kind| known.binary_search(kind).is_err())
        .collect();
    known.extend(new);
    known.sort_unstable();
    (known.len() <= MAX_HEAD_KINDS).then_some(known)
}

/// Whether `expr` or a rule it calls holds a form that `found` picks. Each rule and shared part
/// is looked at once.
pub(crate) fn reaches(
    grammar: &ir::Grammar,
    expr: &ir::Expr,
    found: fn(&ir::Expr) -> bool,
) -> bool {
    let mut seen = vec![false; grammar.rules.len()];
    let mut seen_shared = HashSet::new();
    let mut todo = vec![expr];
    while let Some(expr) = todo.pop() {
        match expr {
            _ if found(expr) => return true,
            ir::Expr::Call(rule) => {
                if let Some(body) = grammar.rules.get(rule.0 as usize).map(|r| &r.body)
                    && !std::mem::replace(&mut seen[rule.0 as usize], true)
                {
                    todo.push(body);
                }
            }
            ir::Expr::Shared(inner) => {
                if seen_shared.insert(std::sync::Arc::as_ptr(inner)) {
                    todo.push(inner);
                }
            }
            _ => todo.extend(expr.children()),
        }
    }
    false
}

impl Graph {
    fn push(&mut self, form: Form, order: Order, need: u32, children: &[u32]) -> u32 {
        let first = self.edges.len() as u32;
        self.edges.extend_from_slice(children);
        self.nodes.push(Node {
            form,
            order,
            need,
            first,
            len: children.len() as u32,
        });
        (self.nodes.len() - 1) as u32
    }

    /// The node of an expression. It recurses as deeply as the expression nests, which the
    /// resolver bounds.
    fn node(&mut self, expr: &ir::Expr) -> u32 {
        use ir::Expr as E;
        let node = match expr {
            E::Empty => self.push(Form::Empty, Order::Start, 0, &[]),
            E::Skip => self.push(Form::Skip, Order::Start, 0, &[]),
            E::Cut => self.push(Form::Cut, Order::Start, 0, &[]),
            E::WordEnd | E::FlagTest { .. } => self.push(Form::Test, Order::Start, 0, &[]),
            E::Same { .. } | E::Ref { .. } => self.push(Form::Unknown, Order::Start, 0, &[]),
            E::Literal { text, .. } if text.is_empty() => {
                self.push(Form::Empty, Order::Start, 0, &[])
            }
            E::Literal {
                text,
                case_insensitive,
            } => {
                let bytes = Bytes::of_literal(text, *case_insensitive);
                self.push(Form::Bytes(bytes), Order::Start, NEVER, &[])
            }
            E::Class(set) => self.push(Form::Bytes(Bytes::of_class(set)), Order::Start, NEVER, &[]),
            E::Any => self.push(Form::Bytes(Bytes::ALL), Order::Start, NEVER, &[]),
            E::ErrorAlternative => self.push(Form::Unknown, Order::Start, NEVER, &[]),
            E::Call(rule) => {
                // The edge is linked to the body once every rule has its node.
                let node = self.push(Form::Call(*rule), Order::Start, 1, &[0]);
                self.calls.push((node, *rule));
                node
            }
            E::Seq(parts) => {
                let children: Vec<u32> = parts.iter().map(|p| self.node(p)).collect();
                let need = children.len() as u32;
                self.push(Form::Sequence, Order::Sequence, need, &children)
            }
            E::Choice(alternatives) => {
                let children: Vec<u32> = alternatives.iter().map(|a| self.node(a)).collect();
                let need = if children.is_empty() { NEVER } else { 1 };
                self.push(Form::Alternatives, Order::Start, need, &children)
            }
            // A repetition of no matches never tries its expression.
            E::Repeat { max: Some(0), .. } => self.push(Form::Empty, Order::Start, 0, &[]),
            E::Repeat { expr, min, .. } => {
                let child = self.node(expr);
                self.push(
                    Form::Alternatives,
                    Order::Start,
                    u32::from(*min > 0),
                    &[child],
                )
            }
            // Lookahead always ends where it started, but what it tests starts there too.
            E::And(expr) | E::Not(expr) => {
                let child = self.node(expr);
                self.push(Form::Test, Order::Start, 0, &[child])
            }
            E::Token { kind, expr, .. } => {
                let child = self.node(expr);
                self.push(Form::Token(*kind), Order::Start, 1, &[child])
            }
            // In recovering mode a label with a recovery may succeed by it instead, matched where
            // the label's expression started, after the skip, so it may match nothing and call
            // rules there. Its edge is linked once every recovery has its node.
            E::Label { label, expr, skip } => {
                let child = self.node(expr);
                if !self
                    .recovered
                    .get(label.0 as usize)
                    .copied()
                    .unwrap_or(false)
                {
                    self.push(Form::Label, Order::Start, 1, &[child])
                } else if *skip {
                    let skip = self.push(Form::Skip, Order::Start, 0, &[]);
                    let after = self.push(Form::Sequence, Order::Sequence, 2, &[skip, 0]);
                    self.label_links
                        .push((self.nodes[after as usize].first + 1, *label));
                    self.push(Form::Label, Order::Start, 1, &[child, after])
                } else {
                    let node = self.push(Form::Label, Order::Start, 1, &[child, 0]);
                    self.label_links
                        .push((self.nodes[node as usize].first + 1, *label));
                    node
                }
            }
            E::FlagOn { expr, .. } | E::FlagOff { expr, .. } => {
                let child = self.node(expr);
                self.push(Form::Flags, Order::Start, 1, &[child])
            }
            E::Indent { expr, .. } | E::Def { expr, .. } => {
                let child = self.node(expr);
                self.push(Form::Unknown, Order::Start, 1, &[child])
            }
            E::Node { expr, .. }
            | E::Transparent { expr, .. }
            | E::Field { expr, .. }
            | E::Unreserved(expr) => {
                let child = self.node(expr);
                self.push(Form::Alternatives, Order::Start, 1, &[child])
            }
            E::Shared(expr) => {
                let key = std::sync::Arc::as_ptr(expr);
                match self.shared.get(&key) {
                    Some(&node) => node,
                    None => {
                        let node = self.node(expr);
                        self.shared.insert(key, node);
                        node
                    }
                }
            }
        };
        self.node_of.insert(std::ptr::from_ref(expr), node);
        node
    }

    /// The children a node starts with, with the role in which each adds to its start: the
    /// parts of a sequence up to the first that cannot succeed without consuming input, those
    /// after a skip or after a part that may end past one as such, and the child of a form
    /// that tries it at its own start. What a lookahead tests is looked at quietly, so it adds
    /// nothing.
    fn roles(&self, node: u32, nullable: &[bool], skips: &[bool]) -> Vec<(u32, Role)> {
        let children = self.children(node);
        match self.nodes[node as usize].form {
            Form::Sequence => {
                let mut roles = Vec::new();
                let mut role = Role::Join;
                for &part in children {
                    if self.nodes[part as usize].form == Form::Skip {
                        role = Role::AfterSkip;
                        continue;
                    }
                    roles.push((part, role));
                    if !nullable[part as usize] {
                        break;
                    }
                    if skips[part as usize] {
                        role = role.past_skip();
                    }
                }
                roles
            }
            Form::Alternatives | Form::Call(_) | Form::Label => {
                children.iter().map(|&child| (child, Role::Join)).collect()
            }
            // A token that cannot match nothing fails at a byte it cannot start with, and records
            // its own failure, whatever stands inside it. One that can match nothing fails only
            // where its expression does without trying a token, which a lookahead can.
            Form::Token(_) => (children.iter())
                .map(|&child| match nullable[child as usize] {
                    true => (child, Role::Join),
                    false => (child, Role::Token),
                })
                .collect(),
            Form::Flags => children.iter().map(|&child| (child, Role::Flags)).collect(),
            Form::Bytes(_) | Form::Empty | Form::Skip | Form::Test | Form::Cut | Form::Unknown => {
                Vec::new()
            }
        }
    }

    /// The children whose head tokens a node tries at its start before it fails there: the
    /// parts of a sequence up to the first that cannot succeed without consuming input, and
    /// the alternatives of a choice up to the first that can, after which none is tried.
    fn head_children(&self, node: u32, nullable: &[bool]) -> Vec<u32> {
        let children = self.children(node);
        let through = |stop: &dyn Fn(u32) -> bool| match children.iter().position(|&c| stop(c)) {
            Some(at) => children[..=at].to_vec(),
            None => children.to_vec(),
        };
        match self.nodes[node as usize].form {
            Form::Sequence => through(&|c| !nullable[c as usize]),
            Form::Alternatives | Form::Call(_) => through(&|c| nullable[c as usize]),
            // A recovery runs only where the label's expression failed, so its tokens are no
            // part of that failure.
            Form::Label => children.iter().take(1).copied().collect(),
            Form::Flags => children.to_vec(),
            Form::Bytes(_)
            | Form::Empty
            | Form::Skip
            | Form::Test
            | Form::Cut
            | Form::Unknown
            | Form::Token(_) => Vec::new(),
        }
    }

    fn children(&self, node: u32) -> &[u32] {
        let n = &self.nodes[node as usize];
        &self.edges[n.first as usize..(n.first + n.len) as usize]
    }

    /// The nodes that have each node as a child.
    fn parents(&self) -> Vec<Vec<u32>> {
        let mut parents: Vec<Vec<u32>> = vec![Vec::new(); self.nodes.len()];
        for node in 0..self.nodes.len() as u32 {
            for &child in self.children(node) {
                parents[child as usize].push(node);
            }
        }
        parents
    }

    /// Which nodes can succeed without consuming input: a node can once as many of its
    /// children can as it needs, so each node is counted down once per child that can.
    fn nullable(&self, parents: &[Vec<u32>]) -> Vec<bool> {
        let mut need: Vec<u32> = self.nodes.iter().map(|n| n.need).collect();
        let mut ready: Vec<u32> = (0..need.len() as u32)
            .filter(|&n| need[n as usize] == 0)
            .collect();
        while let Some(node) = ready.pop() {
            for &parent in &parents[node as usize] {
                let count = &mut need[parent as usize];
                if *count != 0 && *count != NEVER {
                    *count -= 1;
                    if *count == 0 {
                        ready.push(parent);
                    }
                }
            }
        }
        need.into_iter().map(|n| n == 0).collect()
    }

    /// Which nodes can succeed without consuming input past a skip: a skip, an unknown form that
    /// can succeed without consuming input, and a node that can and has a child that can, since
    /// it may end through that child, but for a lookahead, which ends where it started. Each
    /// node is marked once.
    fn skips(&self, parents: &[Vec<u32>], nullable: &[bool]) -> Vec<bool> {
        let unknown = |node: &Node, nullable| node.form == Form::Unknown && nullable;
        let mut skips: Vec<bool> = (self.nodes.iter().zip(nullable))
            .map(|(node, &nullable)| node.form == Form::Skip || unknown(node, nullable))
            .collect();
        let mut todo: Vec<u32> = (0..skips.len() as u32)
            .filter(|&node| skips[node as usize])
            .collect();
        while let Some(node) = todo.pop() {
            for &parent in &parents[node as usize] {
                let form = self.nodes[parent as usize].form;
                if nullable[parent as usize] && form != Form::Test && !skips[parent as usize] {
                    skips[parent as usize] = true;
                    todo.push(parent);
                }
            }
        }
        skips
    }

    /// How many children each node starts at its own start: all of them, or for a sequence
    /// those up to the first that cannot succeed without consuming input. Counted once, since
    /// the search for cycles comes back to a node once per child.
    fn left_counts(&self, nullable: &[bool]) -> Vec<u32> {
        (0..self.nodes.len() as u32)
            .map(|node| {
                let children = self.children(node);
                let count = match self.nodes[node as usize].order {
                    Order::Start => children.len(),
                    Order::Sequence => children
                        .iter()
                        .position(|&c| !nullable[c as usize])
                        .map_or(children.len(), |i| i + 1),
                };
                count as u32
            })
            .collect()
    }

    /// Which nodes lie on a cycle through the first `counts` children of each node, the
    /// strongly connected part of each, and the nodes in the order their parts are complete,
    /// each part after those it reaches, found with Tarjan's algorithm on an explicit stack,
    /// since chains of rules can be as long as the grammar.
    fn cyclic(&self, counts: &[u32]) -> (Vec<bool>, Vec<u32>, Vec<u32>) {
        const UNSEEN: u32 = u32::MAX;
        let count = self.nodes.len();
        let mut index = vec![UNSEEN; count];
        let mut low = vec![0; count];
        let mut on_stack = vec![false; count];
        let mut stack: Vec<u32> = Vec::new();
        let mut cyclic = vec![false; count];
        let mut component = vec![0; count];
        let mut order = Vec::with_capacity(count);
        let mut next_index = 0;
        // Each node being visited with the position of its next child.
        let mut visiting: Vec<(u32, usize)> = Vec::new();
        for root in 0..count as u32 {
            if index[root as usize] != UNSEEN {
                continue;
            }
            index[root as usize] = next_index;
            low[root as usize] = next_index;
            next_index += 1;
            stack.push(root);
            on_stack[root as usize] = true;
            visiting.push((root, 0));
            while let Some(&mut (node, ref mut next)) = visiting.last_mut() {
                let left = &self.children(node)[..counts[node as usize] as usize];
                if let Some(&child) = left.get(*next) {
                    *next += 1;
                    if child == node {
                        cyclic[node as usize] = true;
                    } else if index[child as usize] == UNSEEN {
                        index[child as usize] = next_index;
                        low[child as usize] = next_index;
                        next_index += 1;
                        stack.push(child);
                        on_stack[child as usize] = true;
                        visiting.push((child, 0));
                    } else if on_stack[child as usize] {
                        low[node as usize] = low[node as usize].min(index[child as usize]);
                    }
                    continue;
                }
                visiting.pop();
                if let Some(&(parent, _)) = visiting.last() {
                    low[parent as usize] = low[parent as usize].min(low[node as usize]);
                }
                if low[node as usize] == index[node as usize] {
                    let first = order.len();
                    while let Some(member) = stack.pop() {
                        on_stack[member as usize] = false;
                        component[member as usize] = index[node as usize];
                        order.push(member);
                        if member == node {
                            break;
                        }
                    }
                    if order.len() - first > 1 {
                        for &member in &order[first..] {
                            cyclic[member as usize] = true;
                        }
                    }
                }
            }
        }
        (cyclic, component, order)
    }
}
