// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Facts about a resolved grammar: which rules are left recursive, found with which expressions
//! can succeed without consuming input. Both take time linear in the size of the grammar, over
//! a graph with one node per expression, a shared expression once, and an edge from every call
//! to the body of the rule it calls.

use std::collections::HashMap;

use crate::ir::{self, RuleId};

/// The `need` of a node that never succeeds without consuming input.
const NEVER: u32 = u32::MAX;

/// How the children of a node are matched.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    /// In turn, each where the one before ended.
    Sequence,
    /// Each at the start of the node, or not at all.
    Start,
}

struct Node {
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
}

/// What the compiler needs to know about the rules of a grammar, indexed by rule.
pub(crate) struct Facts {
    /// Whether the rule can call itself where it started, through calls that consume nothing
    /// before them.
    pub left_recursive: Vec<bool>,
}

/// Find the facts of a grammar.
pub(crate) fn analyze(grammar: &ir::Grammar) -> Facts {
    let mut graph = Graph {
        nodes: Vec::new(),
        edges: Vec::new(),
        bodies: Vec::new(),
        calls: Vec::new(),
        shared: HashMap::new(),
    };
    for rule in &grammar.rules {
        let body = graph.node(&rule.body);
        graph.bodies.push(body);
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
    let cyclic = graph.cyclic(&nullable);
    Facts {
        left_recursive: graph.bodies.iter().map(|&b| cyclic[b as usize]).collect(),
    }
}

impl Graph {
    fn push(&mut self, order: Order, need: u32, children: &[u32]) -> u32 {
        let first = self.edges.len() as u32;
        self.edges.extend_from_slice(children);
        self.nodes.push(Node {
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
        match expr {
            E::Empty | E::Cut | E::Skip | E::WordEnd | E::FlagTest { .. } => {
                self.push(Order::Start, 0, &[])
            }
            E::Same { .. } | E::Ref { .. } => self.push(Order::Start, 0, &[]),
            E::Literal { text, .. } if text.is_empty() => self.push(Order::Start, 0, &[]),
            E::Literal { .. } | E::Class(_) | E::Any | E::ErrorAlternative => {
                self.push(Order::Start, NEVER, &[])
            }
            E::Call(rule) => {
                // The edge is linked to the body once every rule has its node.
                let node = self.push(Order::Start, 1, &[0]);
                self.calls.push((node, *rule));
                node
            }
            E::Seq(parts) => {
                let children: Vec<u32> = parts.iter().map(|p| self.node(p)).collect();
                self.push(Order::Sequence, children.len() as u32, &children)
            }
            E::Choice(alternatives) => {
                let children: Vec<u32> = alternatives.iter().map(|a| self.node(a)).collect();
                let need = if children.is_empty() { NEVER } else { 1 };
                self.push(Order::Start, need, &children)
            }
            // A repetition of no matches never tries its expression.
            E::Repeat { max: Some(0), .. } => self.push(Order::Start, 0, &[]),
            E::Repeat { expr, min, .. } => {
                let child = self.node(expr);
                self.push(Order::Start, u32::from(*min > 0), &[child])
            }
            // Lookahead always ends where it started, but what it tests starts there too.
            E::And(expr) | E::Not(expr) => {
                let child = self.node(expr);
                self.push(Order::Start, 0, &[child])
            }
            E::Token { expr, .. }
            | E::Node { expr, .. }
            | E::Transparent { expr, .. }
            | E::Field { expr, .. }
            | E::Unreserved(expr)
            | E::FlagOn { expr, .. }
            | E::FlagOff { expr, .. }
            | E::Indent { expr, .. }
            | E::Def { expr, .. }
            | E::Label { expr, .. } => {
                let child = self.node(expr);
                self.push(Order::Start, 1, &[child])
            }
            E::Shared(expr) => {
                let key = std::sync::Arc::as_ptr(expr);
                if let Some(&node) = self.shared.get(&key) {
                    return node;
                }
                let node = self.node(expr);
                self.shared.insert(key, node);
                node
            }
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

    /// The children a node starts at its own start: all of them, or for a sequence those up to
    /// the first that cannot succeed without consuming input.
    fn left_children<'a>(&'a self, node: u32, nullable: &[bool]) -> &'a [u32] {
        let children = self.children(node);
        match self.nodes[node as usize].order {
            Order::Start => children,
            Order::Sequence => {
                let end = children
                    .iter()
                    .position(|&c| !nullable[c as usize])
                    .map_or(children.len(), |i| i + 1);
                &children[..end]
            }
        }
    }

    /// Which nodes lie on a cycle of left children, found with Tarjan's algorithm on an
    /// explicit stack, since chains of rules can be as long as the grammar.
    fn cyclic(&self, nullable: &[bool]) -> Vec<bool> {
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
                let children = self.left_children(node, nullable);
                if let Some(&child) = children.get(*next) {
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
