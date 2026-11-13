// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! How many items the matches of a resolved grammar add to the nodes they lie in: for every kind
//! of node, how many items each of its fields can hold, and for every rule that can start a
//! parse, whether it always leaves exactly one visible item at the top.
//!
//! Counts are 0, 1 or 2, where 2 stands for several: summed through sequences, the largest
//! through choices, multiplied through repetitions. A node or a token is one item to the node
//! around it, whatever lies inside, so a rule's counts matter where it leaves no node of its own.
//! Rules are counted callees first, the rules that call each other together until their counts
//! stop growing, and a shared part once per pass.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use crate::ir;
use crate::tree::{FieldCount, FieldId};

/// The count that stands for several items.
const SEVERAL: u8 = 2;

/// What one match of an expression adds to the node it lies in, at most.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Items {
    /// Visible items.
    visible: u8,
    /// Visible items without a field, which an enclosing field labels.
    unfielded: u8,
    /// The items of each field.
    fields: BTreeMap<FieldId, u8>,
    /// Whether some match adds no visible item.
    empty: bool,
}

impl Items {
    /// What an expression that never matches adds: the starting point of every count.
    fn never() -> Items {
        Items::default()
    }

    /// What an expression that matches without making an item adds.
    fn nothing() -> Items {
        Items {
            empty: true,
            ..Items::default()
        }
    }

    /// One visible item without a field.
    fn one() -> Items {
        Items {
            visible: 1,
            unfielded: 1,
            fields: BTreeMap::new(),
            empty: false,
        }
    }

    fn then(mut self, next: &Items) -> Items {
        self.visible = sum(self.visible, next.visible);
        self.unfielded = sum(self.unfielded, next.unfielded);
        for (&field, &count) in &next.fields {
            let held = self.fields.entry(field).or_insert(0);
            *held = sum(*held, count);
        }
        self.empty &= next.empty;
        self
    }

    fn or(mut self, other: &Items) -> Items {
        self.visible = self.visible.max(other.visible);
        self.unfielded = self.unfielded.max(other.unfielded);
        for (&field, &count) in &other.fields {
            let held = self.fields.entry(field).or_insert(0);
            *held = (*held).max(count);
        }
        self.empty |= other.empty;
        self
    }

    fn repeated(self, min: u32, max: Option<u32>) -> Items {
        if max == Some(0) {
            return Items::nothing();
        }
        let times = |count: u8| match count {
            0 => 0,
            _ if max == Some(1) => count,
            _ => SEVERAL,
        };
        Items {
            visible: times(self.visible),
            unfielded: times(self.unfielded),
            fields: (self.fields.into_iter())
                .map(|(field, count)| (field, times(count)))
                .collect(),
            empty: min == 0 || self.empty,
        }
    }

    /// The items of a field's match: those without a field get this one (S5).
    fn labelled(mut self, field: FieldId) -> Items {
        if self.unfielded > 0 {
            let held = self.fields.entry(field).or_insert(0);
            *held = sum(*held, self.unfielded);
            self.unfielded = 0;
        }
        self
    }
}

fn sum(a: u8, b: u8) -> u8 {
    (a + b).min(SEVERAL)
}

/// Counts expressions, with what each rule adds as far as known.
struct Counter<'r> {
    rules: &'r [Items],
    /// The shared parts counted in this pass.
    shared: HashMap<*const ir::Expr, Items>,
}

impl Counter<'_> {
    fn new(rules: &[Items]) -> Counter<'_> {
        Counter {
            rules,
            shared: HashMap::new(),
        }
    }

    /// What a match of the expression adds to the node around it. It recurses as deeply as the
    /// expression nests, which the resolver bounds.
    fn of(&mut self, expr: &ir::Expr) -> Items {
        use ir::Expr as E;
        match expr {
            E::Token { hidden: true, .. } => Items::nothing(),
            E::Token { .. } | E::Node { .. } => Items::one(),
            // A transparent node stays a node, or gives way to its one visible child, which
            // keeps any field it has from inside the rule.
            E::Transparent { expr, .. } => {
                let inner = self.of(expr);
                Items {
                    fields: inner.fields.keys().map(|&field| (field, 1)).collect(),
                    ..Items::one()
                }
            }
            E::Call(rule) => (self.rules.get(rule.0 as usize).cloned()).unwrap_or_default(),
            E::Seq(parts) => parts.iter().fold(Items::nothing(), |items, part| {
                let part = self.of(part);
                items.then(&part)
            }),
            E::Choice(alternatives) => alternatives.iter().fold(Items::never(), |items, a| {
                let alternative = self.of(a);
                items.or(&alternative)
            }),
            E::Repeat { expr, min, max } => self.of(expr).repeated(*min, *max),
            E::Field { field, expr } => self.of(expr).labelled(*field),
            // What a lookahead makes is dropped.
            E::And(_) | E::Not(_) => Items::nothing(),
            E::Unreserved(expr)
            | E::FlagOn { expr, .. }
            | E::FlagOff { expr, .. }
            | E::Indent { expr, .. }
            | E::Def { expr, .. }
            | E::Label { expr, .. } => self.of(expr),
            E::Shared(expr) => {
                let key = Arc::as_ptr(expr);
                if let Some(items) = self.shared.get(&key) {
                    return items.clone();
                }
                let items = self.of(expr);
                self.shared.insert(key, items.clone());
                items
            }
            E::ErrorAlternative => Items::never(),
            // Atoms outside a token match without an item, inside a terminal.
            E::Empty
            | E::Literal { .. }
            | E::Class(_)
            | E::Any
            | E::Cut
            | E::Skip
            | E::WordEnd
            | E::FlagTest { .. }
            | E::Same { .. }
            | E::Ref { .. } => Items::nothing(),
        }
    }
}

/// The item counts of a resolved grammar. See the module documentation.
#[derive(Debug, Default)]
pub(crate) struct ItemFacts {
    /// For each kind, by kind id, the fields of its nodes with how many items each can hold.
    pub kind_fields: Vec<Vec<(FieldId, FieldCount)>>,
    /// For each rule that can start a parse, whether a parse from it always leaves exactly one
    /// visible item at the top.
    pub single_item: HashMap<String, bool>,
}

/// What each rule of a grammar adds where it is called, from which the counts of any
/// expression follow. A rule missing from the slice, which only a grammar with errors has, never
/// matches.
pub(crate) struct Counts {
    summaries: Vec<Items>,
}

impl Counts {
    /// Count the rules, callees first.
    pub fn new(rules: &[Option<ir::Rule>]) -> Counts {
        Counts {
            summaries: summarize(rules),
        }
    }

    /// The fields that one match of `expr` can give items to, in the node around it.
    pub fn fields(&self, expr: &ir::Expr) -> Vec<FieldId> {
        let items = Counter::new(&self.summaries).of(expr);
        (items.fields.into_iter())
            .filter(|&(_, count)| count > 0)
            .map(|(field, _)| field)
            .collect()
    }

    /// The facts of the grammar, with `kinds` kinds in its kind table.
    pub fn facts(
        &self,
        rules: &[Option<ir::Rule>],
        entry: &HashMap<String, ir::Expr>,
        kinds: usize,
    ) -> ItemFacts {
        let mut counter = Counter::new(&self.summaries);
        let mut fields = vec![BTreeMap::new(); kinds];
        let mut walked = HashSet::new();
        for rule in rules.iter().flatten() {
            node_fields(&mut counter, &rule.body, &mut fields, &mut walked);
        }
        let kind_fields = (fields.into_iter())
            .map(|counts: BTreeMap<FieldId, u8>| {
                (counts.into_iter())
                    .filter(|&(_, count)| count > 0)
                    .map(|(field, count)| {
                        let count = if count == 1 {
                            FieldCount::One
                        } else {
                            FieldCount::Many
                        };
                        (field, count)
                    })
                    .collect()
            })
            .collect();
        let single_item = (entry.iter())
            .map(|(name, expr)| {
                let items = counter.of(expr);
                (name.clone(), items.visible <= 1 && !items.empty)
            })
            .collect();
        ItemFacts {
            kind_fields,
            single_item,
        }
    }
}

/// Add the field counts of every node in the expression to its kind, a shared part once.
fn node_fields(
    counter: &mut Counter<'_>,
    expr: &ir::Expr,
    fields: &mut [BTreeMap<FieldId, u8>],
    walked: &mut HashSet<*const ir::Expr>,
) {
    use ir::Expr as E;
    match expr {
        E::Node { kind, expr: inner } | E::Transparent { kind, expr: inner } => {
            let items = counter.of(inner);
            if let Some(counts) = fields.get_mut(kind.0 as usize) {
                for (field, count) in items.fields {
                    let held = counts.entry(field).or_insert(0);
                    *held = (*held).max(count);
                }
            }
            node_fields(counter, inner, fields, walked);
        }
        E::Shared(inner) => {
            if walked.insert(Arc::as_ptr(inner)) {
                node_fields(counter, inner, fields, walked);
            }
        }
        _ => {
            for child in expr.children() {
                node_fields(counter, child, fields, walked);
            }
        }
    }
}

/// What each rule adds where it is called, rules that call each other counted together until
/// their counts stop growing. Counts only grow from `never`, so this ends.
fn summarize(rules: &[Option<ir::Rule>]) -> Vec<Items> {
    let calls: Vec<Vec<usize>> = rules
        .iter()
        .map(|rule| {
            let mut calls = Vec::new();
            if let Some(rule) = rule {
                item_calls(&rule.body, rules.len(), &mut calls, &mut HashSet::new());
            }
            calls
        })
        .collect();
    let mut summaries = vec![Items::never(); rules.len()];
    for component in components(&calls) {
        let recursive = component.len() > 1 || calls[component[0]].contains(&component[0]);
        loop {
            let mut changed = false;
            for &rule in &component {
                let Some(body) = rules[rule].as_ref().map(|rule| &rule.body) else {
                    continue;
                };
                let items = Counter::new(&summaries).of(body);
                if items != summaries[rule] {
                    summaries[rule] = items;
                    changed = true;
                }
            }
            if !changed || !recursive {
                break;
            }
        }
    }
    summaries
}

/// The rules an expression calls where what they make joins its items: not inside a node, a
/// token or a lookahead.
fn item_calls(
    expr: &ir::Expr,
    rules: usize,
    calls: &mut Vec<usize>,
    walked: &mut HashSet<*const ir::Expr>,
) {
    use ir::Expr as E;
    match expr {
        E::Node { .. } | E::Token { .. } | E::And(_) | E::Not(_) => {}
        E::Call(rule) if (rule.0 as usize) < rules => calls.push(rule.0 as usize),
        E::Shared(inner) => {
            if walked.insert(Arc::as_ptr(inner)) {
                item_calls(inner, rules, calls, walked);
            }
        }
        _ => {
            for child in expr.children() {
                item_calls(child, rules, calls, walked);
            }
        }
    }
}

/// The strongly connected components of a call graph, each after every component it calls,
/// found with Tarjan's algorithm on an explicit stack, since chains of calls can be as long as
/// the grammar.
fn components(calls: &[Vec<usize>]) -> Vec<Vec<usize>> {
    const UNSEEN: usize = usize::MAX;
    let count = calls.len();
    let mut index = vec![UNSEEN; count];
    let mut low = vec![0; count];
    let mut on_stack = vec![false; count];
    let mut stack = Vec::new();
    let mut found = Vec::new();
    let mut next_index = 0;
    // Each rule being visited with the position of its next callee.
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
        while let Some(&mut (rule, ref mut next)) = visiting.last_mut() {
            if let Some(&callee) = calls[rule].get(*next) {
                *next += 1;
                if index[callee] == UNSEEN {
                    index[callee] = next_index;
                    low[callee] = next_index;
                    next_index += 1;
                    stack.push(callee);
                    on_stack[callee] = true;
                    visiting.push((callee, 0));
                } else if on_stack[callee] {
                    low[rule] = low[rule].min(index[callee]);
                }
                continue;
            }
            visiting.pop();
            if let Some(&(caller, _)) = visiting.last() {
                low[caller] = low[caller].min(low[rule]);
            }
            if low[rule] == index[rule] {
                let mut members = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    members.push(member);
                    if member == rule {
                        break;
                    }
                }
                found.push(members);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use crate::builder::{GrammarBuilder, class, field, lit, opt, r, range, sep_many, seq, some};
    use crate::tree::FieldCount::{Many, One};
    use crate::{FieldCount, Grammar, Parser};

    /// The fields of the kind named `kind`, by field name.
    fn fields(grammar: &Grammar, kind: &str) -> Vec<(String, FieldCount)> {
        let parser = Parser::new(grammar).unwrap_or_else(|e| panic!("{e:?}"));
        let names = parser.names();
        let mut out: Vec<(String, FieldCount)> = (parser.kinds().iter())
            .filter(|k| k.name == kind)
            .flat_map(|k| k.fields.iter())
            .map(|&(field, count)| (names.fields[usize::from(field.0)].clone(), count))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    fn word() -> crate::Expr {
        some(class([range('a', 'z')]))
    }

    #[test]
    fn fields_hold_one_item_unless_repeated_or_written_twice() {
        let mut g = GrammarBuilder::new();
        g.rule(
            "call",
            seq([
                field("name", r("NAME")),
                lit("("),
                field("args", sep_many(r("NAME"), lit(","))),
                lit(")"),
                opt(field("kw", r("NAME"))),
                field("twice", r("NAME")),
                field("twice", r("NAME")),
            ]),
        );
        g.rule("NAME", word());
        let grammar = g.build();
        let counts = fields(&grammar, "call");
        let expected = [("args", Many), ("kw", One), ("name", One), ("twice", Many)];
        let expected: Vec<(String, FieldCount)> =
            expected.iter().map(|&(n, c)| (n.to_owned(), c)).collect();
        assert_eq!(counts, expected);
    }

    #[test]
    fn fields_of_rules_without_a_node_join_the_node_that_calls_them() {
        let grammar = Grammar::from_text(
            "pair: _kv\n_kv: k=NAME v=NAME\nlist: _items\n_items: x=NAME _items | ()\n\
             ?wrap: _k\n_k: k=NAME\nouter: w=wrap\nNAME: [a-z]+\n%ignore \" \"+",
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let named = |kind| fields(&grammar, kind);
        assert_eq!(
            named("pair"),
            [("k".to_owned(), One), ("v".to_owned(), One)]
        );
        assert_eq!(named("list"), [("x".to_owned(), Many)]);
        // A transparent node that gives way to its child passes the child's field up.
        assert_eq!(
            named("outer"),
            [("k".to_owned(), One), ("w".to_owned(), One)]
        );
    }

    #[test]
    fn a_start_rule_leaves_one_item_unless_it_leaves_no_node_of_its_own() {
        let grammar = Grammar::from_text(
            "start: A B\n?clear: A | A B\n_pair: A B\n_maybe: A?\nA: \"a\"\n_B: \"b\"\nB: \"b\"",
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let parser = Parser::new(&grammar).unwrap_or_else(|e| panic!("{e:?}"));
        let single = |name| parser.single_item(name);
        assert_eq!(single("start"), Some(true));
        assert_eq!(single("clear"), Some(true));
        assert_eq!(single("A"), Some(true));
        assert_eq!(single("_pair"), Some(false));
        assert_eq!(single("_maybe"), Some(false));
        assert_eq!(single("_B"), Some(false));
        assert_eq!(single("missing"), None);
    }
}
