// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The strongly connected components of a graph, which the analyses of rules and of their items
//! both work through.

/// The strongly connected components of a graph, each after every component it reaches.
pub(crate) struct Components {
    /// The nodes, component by component, each component in the order its nodes left the stack.
    pub order: Vec<usize>,
    /// Where each component ends in `order`.
    ends: Vec<usize>,
}

impl Components {
    /// The nodes of each component, in order.
    pub fn iter(&self) -> impl Iterator<Item = &[usize]> {
        let starts = std::iter::once(0).chain(self.ends.iter().copied());
        starts
            .zip(&self.ends)
            .map(|(start, &end)| &self.order[start..end])
    }
}

/// The strongly connected components of a graph of `count` nodes, where `successor(node, i)` is
/// the `i`th successor of a node, `None` past the last. Tarjan's algorithm on an explicit stack,
/// since chains of rules can be as long as the grammar.
pub(crate) fn components(
    count: usize,
    successor: impl Fn(usize, usize) -> Option<usize>,
) -> Components {
    const UNSEEN: usize = usize::MAX;
    let mut index = vec![UNSEEN; count];
    let mut low = vec![0; count];
    let mut on_stack = vec![false; count];
    let mut stack = Vec::new();
    let mut found = Components {
        order: Vec::with_capacity(count),
        ends: Vec::new(),
    };
    let mut next_index = 0;
    // Each node being visited with the position of its next successor.
    let mut visiting: Vec<(usize, usize)> = Vec::new();
    for root in 0..count {
        if index[root] != UNSEEN {
            continue;
        }
        index[root] = next_index;
        low[root] = next_index;
        next_index += 1;
        stack.push(root);
        on_stack[root] = true;
        visiting.push((root, 0));
        while let Some(&mut (node, ref mut next)) = visiting.last_mut() {
            if let Some(child) = successor(node, *next) {
                *next += 1;
                if index[child] == UNSEEN {
                    index[child] = next_index;
                    low[child] = next_index;
                    next_index += 1;
                    stack.push(child);
                    on_stack[child] = true;
                    visiting.push((child, 0));
                } else if on_stack[child] {
                    low[node] = low[node].min(index[child]);
                }
                continue;
            }
            visiting.pop();
            if let Some(&(parent, _)) = visiting.last() {
                low[parent] = low[parent].min(low[node]);
            }
            if low[node] == index[node] {
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    found.order.push(member);
                    if member == node {
                        break;
                    }
                }
                found.ends.push(found.order.len());
            }
        }
    }
    found
}
