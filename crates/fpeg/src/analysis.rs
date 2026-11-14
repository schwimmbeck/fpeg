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
    /// Columns, captures and the alternative for errors, which the machine does not run yet.
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
        after_skip: true,
        at_once: true,
        plain: false,
        tokens: true,
        clean: false,
        growing: true,
    };

    fn merge(self, other: Start) -> Start {
        Start {
            bytes: self.bytes.union(other.bytes),
            nullable: self.nullable || other.nullable,
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
    };
    for rule in &grammar.rules {
        let body = graph.node(&rule.body);
        graph.bodies.push(body);
    }
    // The skip expression and the entries are compiled too.
    for expr in grammar.skip.iter().chain(grammar.entry.values()) {
        graph.node(expr);
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
    let nullable = graph.nullable();
    let left = graph.cyclic(&graph.left_counts(&nullable));
    let all: Vec<u32> = graph.nodes.iter().map(|node| node.len).collect();
    let anywhere = graph.cyclic(&all);
    Facts {
        left_recursive: graph.bodies.iter().map(|&b| left[b as usize]).collect(),
        recursive: graph.bodies.iter().map(|&b| anywhere[b as usize]).collect(),
        graph,
        nullable,
    }
}

impl Facts {
    /// What every expression can start with. A node's start is what it adds itself, joined
    /// with what each child it starts with adds through its role. Starts only grow, each a
    /// bounded number of times, by a byte or a fact, and each growth is passed once along
    /// every edge to a parent, so the time stays linear.
    ///
    /// Where the skip expression tests a flag, a skip under a flag turned on or off may end
    /// elsewhere than the skip before the flag changed, so what follows it may start with any
    /// byte.
    ///
    /// `growing` says which rules keep the growing loop.
    pub fn starts(&self, grammar: &ir::Grammar, growing: &[bool]) -> Starts<'_> {
        let graph = &self.graph;
        let skip_reads_flags = (grammar.skip.as_ref()).is_some_and(|s| reads_flags(grammar, s));
        let mut parents: Vec<Vec<(u32, Role)>> = vec![Vec::new(); graph.nodes.len()];
        for node in 0..graph.nodes.len() as u32 {
            for (child, role) in graph.roles(node, &self.nullable) {
                parents[child as usize].push((node, role));
            }
        }
        let own = |node: &Node| node.own_start(growing);
        let mut values: Vec<Start> = graph.nodes.iter().map(own).collect();
        let mut queued = vec![true; graph.nodes.len()];
        let mut todo: Vec<u32> = (0..graph.nodes.len() as u32).collect();
        while let Some(child) = todo.pop() {
            queued[child as usize] = false;
            let start = values[child as usize];
            for &(parent, role) in &parents[child as usize] {
                let added = match role {
                    Role::Join => start,
                    Role::AfterSkip => Start {
                        after_skip: start.after_skip || start.at_once,
                        at_once: false,
                        ..start
                    },
                    Role::Flags if skip_reads_flags && start.after_skip => Start {
                        bytes: Bytes::ALL,
                        ..start
                    },
                    Role::Flags => start,
                    Role::Token => Start {
                        clean: true,
                        ..start
                    },
                };
                let joined = values[parent as usize].merge(added);
                if joined != values[parent as usize] {
                    values[parent as usize] = joined;
                    if !std::mem::replace(&mut queued[parent as usize], true) {
                        todo.push(parent);
                    }
                }
            }
        }
        for (start, &nullable) in values.iter_mut().zip(&self.nullable) {
            start.nullable = nullable;
        }
        Starts {
            node_of: &graph.node_of,
            values,
            heads: self.heads(),
        }
    }

    /// The kinds of the head tokens of every node, found as the starts are: each node adds
    /// its own, a token the kind of its own unless it can match nothing, and joins those of
    /// the children it tries at its start before it fails there.
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
        let mut todo: Vec<u32> = (0..graph.nodes.len() as u32).collect();
        while let Some(child) = todo.pop() {
            queued[child as usize] = false;
            for &parent in &parents[child as usize] {
                let joined = match (&heads[parent as usize], &heads[child as usize]) {
                    (Some(known), Some(added)) => {
                        let mut joined = known.clone();
                        joined.extend(
                            added
                                .iter()
                                .filter(|kind| known.binary_search(kind).is_err()),
                        );
                        joined.sort_unstable();
                        (joined.len() <= MAX_HEAD_KINDS).then_some(joined)
                    }
                    _ => None,
                };
                if joined != heads[parent as usize] {
                    heads[parent as usize] = joined;
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
    /// Under a flag turned on or off.
    Flags,
    /// Inside a token that cannot match nothing.
    Token,
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

/// Whether `expr` or a rule it calls tests a flag. Each rule and shared part is looked at once.
fn reads_flags(grammar: &ir::Grammar, expr: &ir::Expr) -> bool {
    let mut seen = vec![false; grammar.rules.len()];
    let mut seen_shared = HashSet::new();
    let mut todo = vec![expr];
    while let Some(expr) = todo.pop() {
        match expr {
            ir::Expr::FlagTest { .. } => return true,
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
            E::Label { expr, .. } => {
                let child = self.node(expr);
                self.push(Form::Label, Order::Start, 1, &[child])
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
    /// after a skip as such, and the child of a form that tries it at its own start. What a
    /// lookahead tests is looked at quietly, so it adds nothing.
    fn roles(&self, node: u32, nullable: &[bool]) -> Vec<(u32, Role)> {
        let children = self.children(node);
        match self.nodes[node as usize].form {
            Form::Sequence => {
                let mut roles = Vec::new();
                let mut after_skip = false;
                for &part in children {
                    if self.nodes[part as usize].form == Form::Skip {
                        after_skip = true;
                        continue;
                    }
                    roles.push((
                        part,
                        if after_skip {
                            Role::AfterSkip
                        } else {
                            Role::Join
                        },
                    ));
                    if !nullable[part as usize] {
                        break;
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
            Form::Label | Form::Flags => children.to_vec(),
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

    /// Which nodes can succeed without consuming input: a node can once as many of its
    /// children can as it needs, so each node is counted down once per child that can.
    fn nullable(&self) -> Vec<bool> {
        let mut parents: Vec<Vec<u32>> = vec![Vec::new(); self.nodes.len()];
        for node in 0..self.nodes.len() as u32 {
            for &child in self.children(node) {
                parents[child as usize].push(node);
            }
        }
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

    /// Which nodes lie on a cycle through the first `counts` children of each node, found
    /// with Tarjan's algorithm on an explicit stack, since chains of rules can be as long as
    /// the grammar.
    fn cyclic(&self, counts: &[u32]) -> Vec<bool> {
        const UNSEEN: u32 = u32::MAX;
        let count = self.nodes.len();
        let mut index = vec![UNSEEN; count];
        let mut low = vec![0; count];
        let mut on_stack = vec![false; count];
        let mut stack: Vec<u32> = Vec::new();
        let mut cyclic = vec![false; count];
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
                    let mut members = Vec::new();
                    while let Some(member) = stack.pop() {
                        on_stack[member as usize] = false;
                        members.push(member);
                        if member == node {
                            break;
                        }
                    }
                    if members.len() > 1 {
                        for member in members {
                            cyclic[member as usize] = true;
                        }
                    }
                }
            }
        }
        cyclic
    }
}
