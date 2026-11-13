// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! How many items the matches of a resolved grammar add to the nodes they lie in: for every kind
//! of node, how many items each of its fields can hold, and for every rule that can start a
//! parse, whether it always leaves exactly one visible item at the top.
//!
//! Counts are 0, 1 or 2, where 2 stands for several: summed through sequences, the largest
//! through choices, multiplied through repetitions. A node or a token is one item to the node
//! around it, whatever lies inside, so a rule's counts matter where it leaves no node of its own.
//! Beside the most items, the counts know whether a match can add no visible item or exactly
//! one, which tells what becomes of a transparent node, and the fields that every match fills,
//! so that a field of one item tells whether it can be absent.
//!
//! Rules are counted callees first, the rules that call each other together until their counts
//! stop growing, and a shared part once per pass. The fields every match fills are found after
//! the most items are known: from every field a match can fill, dropping those that some
//! alternative leaves out until none is dropped. Any set that this leaves stable holds for every
//! finite match, by induction over the match, so the largest one is the most precise answer.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use crate::ir;
use crate::tree::{FieldCount, FieldId, KindId};

/// The count that stands for several items.
const SEVERAL: u8 = 2;

/// The most fields that a rule without a node of its own may pass on to the node around it.
/// A chain of such rules passes on the fields of all below, so counting them takes time and
/// memory that grow with the square of the chain, and a grammar beyond this is refused.
pub(crate) const MAX_NODE_FIELDS: usize = 1_000;

/// The most steps that counting the items of a grammar may take, a step being an expression
/// counted or a field it passes on. Rules without a node of their own that call each other
/// pass fields around until the counts stop growing, which can take a pass for every rule of
/// the cycle, and a grammar beyond this is refused.
pub(crate) const MAX_COUNT_STEPS: usize = 10_000_000;

/// The limit a grammar exceeded while its items were counted, with the rule being counted.
#[derive(Debug)]
pub(crate) enum TooLarge {
    /// The rule passes on more than [`MAX_NODE_FIELDS`] fields.
    Fields(usize),
    /// Counting took more than [`MAX_COUNT_STEPS`] steps.
    Steps(usize),
}

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
    /// Whether some match adds exactly one visible item.
    single: bool,
    /// The fields that every match gives an item, and whether every match adds a visible item
    /// without a field.
    present: BTreeSet<FieldId>,
    unfielded_present: bool,
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
            single: true,
            unfielded_present: true,
            ..Items::default()
        }
    }

    /// Whether the expression can match at all, as far as the counts show.
    fn matches(&self) -> bool {
        self.visible > 0 || self.empty
    }

    /// Whether a transparent node around the expression stays for some match: one that adds
    /// no visible item, or several.
    fn stays(&self) -> bool {
        self.visible > 1 || self.empty
    }

    /// Whether two counts agree on the most items, leaving out the fields every match fills.
    fn same_most(&self, other: &Items) -> bool {
        let most = |items: &Items| (items.visible, items.unfielded, items.empty, items.single);
        most(self) == most(other) && self.fields == other.fields
    }

    fn then(mut self, next: &Items) -> Items {
        self.visible = sum(self.visible, next.visible);
        self.unfielded = sum(self.unfielded, next.unfielded);
        for (&field, &count) in &next.fields {
            let held = self.fields.entry(field).or_insert(0);
            *held = sum(*held, count);
        }
        // One item in all: from one side, with none from the other.
        self.single = (self.single && next.empty) || (self.empty && next.single);
        self.empty &= next.empty;
        self.present.extend(next.present.iter().copied());
        self.unfielded_present |= next.unfielded_present;
        self
    }

    fn or(mut self, other: &Items) -> Items {
        // An alternative that never matches leaves the fields every match fills as they are.
        if !self.matches() {
            self.present.clone_from(&other.present);
            self.unfielded_present = other.unfielded_present;
        } else if other.matches() {
            self.present.retain(|field| other.present.contains(field));
            self.unfielded_present &= other.unfielded_present;
        }
        self.visible = self.visible.max(other.visible);
        self.unfielded = self.unfielded.max(other.unfielded);
        for (&field, &count) in &other.fields {
            let held = self.fields.entry(field).or_insert(0);
            *held = (*held).max(count);
        }
        self.empty |= other.empty;
        self.single |= other.single;
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
            // One step that adds one item, and any further steps that add none.
            single: self.single && (min <= 1 || self.empty),
            present: if min > 0 {
                self.present
            } else {
                BTreeSet::new()
            },
            unfielded_present: min > 0 && self.unfielded_present,
        }
    }

    /// The items of a field's match: those without a field get this one (S5).
    fn labelled(mut self, field: FieldId) -> Items {
        if self.unfielded > 0 {
            let held = self.fields.entry(field).or_insert(0);
            *held = sum(*held, self.unfielded);
            self.unfielded = 0;
        }
        if self.unfielded_present {
            self.present.insert(field);
        }
        self.unfielded_present = false;
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
    /// The steps taken, which count against [`MAX_COUNT_STEPS`].
    steps: usize,
}

impl Counter<'_> {
    fn new(rules: &[Items]) -> Counter<'_> {
        Counter {
            rules,
            shared: HashMap::new(),
            steps: 0,
        }
    }

    /// What a match of the expression adds to the node around it. It recurses as deeply as the
    /// expression nests, which the resolver bounds.
    fn of(&mut self, expr: &ir::Expr) -> Items {
        let items = self.count(expr);
        // The expression around this one takes its fields over once.
        self.steps += 1 + items.fields.len() + items.present.len();
        items
    }

    fn count(&mut self, expr: &ir::Expr) -> Items {
        use ir::Expr as E;
        match expr {
            E::Token { hidden: true, .. } => Items::nothing(),
            E::Token { .. } | E::Node { .. } => Items::one(),
            // A transparent node stays a node, or gives way to its one visible child, which
            // keeps any field it has from inside the rule. So no field is sure to be filled,
            // and the item is sure to have no field only when nothing inside has one.
            E::Transparent { expr, .. } => {
                let inner = self.of(expr);
                Items {
                    unfielded_present: inner.fields.is_empty(),
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
    /// For each kind, by kind id, whether a parse can make items of it.
    pub kind_made: Vec<bool>,
    /// For each rule that can start a parse, whether a parse from it always leaves exactly one
    /// visible item at the top.
    pub single_item: HashMap<String, bool>,
}

/// What becomes of a transparent node, as far as the counts of the expression in it show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shape {
    /// Some match leaves no visible item or several, and the node stays.
    pub stays: bool,
    /// Some match leaves exactly one visible item, and the node gives way to it.
    pub gives_way: bool,
}

/// What each rule of a grammar adds where it is called, from which the counts of any
/// expression follow. A rule missing from the slice, which only a grammar with errors has, never
/// matches.
pub(crate) struct Counts {
    summaries: Vec<Items>,
}

impl Counts {
    /// Count the rules, callees first, or tell the limit that the grammar exceeds.
    pub fn new(rules: &[Option<ir::Rule>]) -> Result<Counts, TooLarge> {
        Ok(Counts {
            summaries: summarize(rules)?,
        })
    }

    /// The fields that one match of `expr` can give items to, in the node around it.
    pub fn fields(&self, expr: &ir::Expr) -> Vec<FieldId> {
        let items = Counter::new(&self.summaries).of(expr);
        (items.fields.into_iter())
            .filter(|&(_, count)| count > 0)
            .map(|(field, _)| field)
            .collect()
    }

    /// What becomes of a transparent node around `expr`.
    pub fn shape(&self, expr: &ir::Expr) -> Shape {
        let items = Counter::new(&self.summaries).of(expr);
        Shape {
            stays: items.stays(),
            gives_way: items.single,
        }
    }

    /// The facts of the grammar, with `kinds` kinds in its kind table.
    pub fn facts(
        &self,
        rules: &[Option<ir::Rule>],
        entry: &HashMap<String, ir::Expr>,
        kinds: usize,
    ) -> ItemFacts {
        let mut counter = Counter::new(&self.summaries);
        let mut kind_items = KindItems {
            fields: vec![BTreeMap::new(); kinds],
            present: vec![None; kinds],
            made: vec![false; kinds],
        };
        let mut walked = HashSet::new();
        // The entries add the tokens of terminals that only other terminals use.
        let bodies = rules.iter().flatten().map(|rule| &rule.body);
        for expr in bodies.chain(entry.values()) {
            kind_items.add(&mut counter, expr, &mut walked);
        }
        let KindItems {
            fields,
            present,
            made,
        } = kind_items;
        let kind_fields = (fields.into_iter().zip(present))
            .map(|(counts, present)| {
                let present = present.unwrap_or_default();
                (counts.into_iter())
                    .filter(|&(_, count)| count > 0)
                    .map(|(field, count)| {
                        let count = match count {
                            1 if present.contains(&field) => FieldCount::One,
                            1 => FieldCount::Optional,
                            _ => FieldCount::Many,
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
            kind_made: made,
            single_item,
        }
    }
}

/// What the kinds of a grammar hold, by kind id: the counts of each field, the fields that every
/// node of the kind fills, if the kind makes nodes, and whether a parse can make items of it.
struct KindItems {
    fields: Vec<BTreeMap<FieldId, u8>>,
    present: Vec<Option<BTreeSet<FieldId>>>,
    made: Vec<bool>,
}

impl KindItems {
    /// Add the tokens and nodes of an expression, a shared part once. A transparent node whose
    /// every match leaves exactly one visible item never stays: that item takes its place.
    fn add(
        &mut self,
        counter: &mut Counter<'_>,
        expr: &ir::Expr,
        walked: &mut HashSet<*const ir::Expr>,
    ) {
        use ir::Expr as E;
        match expr {
            E::Token {
                kind, expr: inner, ..
            } => {
                self.mark(*kind);
                self.add(counter, inner, walked);
            }
            E::Node { kind, expr: inner } | E::Transparent { kind, expr: inner } => {
                let items = counter.of(inner);
                // Only a node that stays holds fields: an alternative of a transparent rule that
                // gives way to its one child passes the child's field on instead.
                if matches!(expr, E::Node { .. }) || items.stays() {
                    self.mark(*kind);
                    if let Some(present) = self.present.get_mut(kind.0 as usize) {
                        match present {
                            Some(fields) => fields.retain(|field| items.present.contains(field)),
                            None => *present = Some(items.present.clone()),
                        }
                    }
                    if let Some(counts) = self.fields.get_mut(kind.0 as usize) {
                        for (field, count) in items.fields {
                            let held = counts.entry(field).or_insert(0);
                            *held = (*held).max(count);
                        }
                    }
                }
                self.add(counter, inner, walked);
            }
            E::Shared(inner) => {
                if walked.insert(Arc::as_ptr(inner)) {
                    self.add(counter, inner, walked);
                }
            }
            _ => {
                for child in expr.children() {
                    self.add(counter, child, walked);
                }
            }
        }
    }

    fn mark(&mut self, kind: KindId) {
        if let Some(made) = self.made.get_mut(kind.0 as usize) {
            *made = true;
        }
    }
}

/// What each rule adds where it is called, rules that call each other counted together until
/// their counts stop growing. Counts only grow from `never`, so this ends.
fn summarize(rules: &[Option<ir::Rule>]) -> Result<Vec<Items>, TooLarge> {
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
    let mut steps = 0;
    for component in components(&calls) {
        let recursive = component.len() > 1 || calls[component[0]].contains(&component[0]);
        count_until_stable(
            &component,
            rules,
            &mut summaries,
            &mut steps,
            recursive,
            Items::same_most,
        )?;
        if recursive {
            // The fields every match fills, from all a match can fill down, while the most items
            // stay as they are and with them the alternatives that can match.
            for &rule in &component {
                let summary = &mut summaries[rule];
                let filled = summary.fields.iter().filter(|&(_, &count)| count > 0);
                summary.present = filled.map(|(&field, _)| field).collect();
                summary.unfielded_present = summary.unfielded > 0;
            }
            count_until_stable(
                &component,
                rules,
                &mut summaries,
                &mut steps,
                true,
                Items::eq,
            )?;
        }
    }
    Ok(summaries)
}

/// Count the rules of a component again and again, while `same` finds a count changed, or once
/// unless `repeat`. Stops at a rule that passes on more than [`MAX_NODE_FIELDS`] fields, or
/// when `steps` passes [`MAX_COUNT_STEPS`].
fn count_until_stable(
    component: &[usize],
    rules: &[Option<ir::Rule>],
    summaries: &mut [Items],
    steps: &mut usize,
    repeat: bool,
    same: fn(&Items, &Items) -> bool,
) -> Result<(), TooLarge> {
    loop {
        let mut changed = false;
        for &rule in component {
            let Some(body) = rules[rule].as_ref().map(|rule| &rule.body) else {
                continue;
            };
            let mut counter = Counter::new(summaries);
            let items = counter.of(body);
            *steps += counter.steps;
            if items.fields.len() > MAX_NODE_FIELDS {
                return Err(TooLarge::Fields(rule));
            }
            if *steps > MAX_COUNT_STEPS {
                return Err(TooLarge::Steps(rule));
            }
            changed |= !same(&items, &summaries[rule]);
            summaries[rule] = items;
        }
        if !changed || !repeat {
            return Ok(());
        }
    }
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
    use crate::tree::FieldCount::{Many, One, Optional};
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
    fn fields_hold_one_item_unless_optional_repeated_or_written_twice() {
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
        let expected = [
            ("args", Many),
            ("kw", Optional),
            ("name", One),
            ("twice", Many),
        ];
        let expected: Vec<(String, FieldCount)> =
            expected.iter().map(|&(n, c)| (n.to_owned(), c)).collect();
        assert_eq!(counts, expected);
    }

    #[test]
    fn fields_of_rules_without_a_node_join_the_node_that_calls_them() {
        let grammar = Grammar::from_text(
            "pair: _kv\n_kv: k=NAME v=NAME\nlist: _items\n_items: x=NAME _items | ()\n\
             ?wrap: _k\n_k: k=NAME\nouter: w=wrap\n?two: NAME NAME | _k\n\
             NAME: [a-z]+\n%ignore \" \"+",
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let named = |kind| fields(&grammar, kind);
        assert_eq!(
            named("pair"),
            [("k".to_owned(), One), ("v".to_owned(), One)]
        );
        assert_eq!(named("list"), [("x".to_owned(), Many)]);
        // A transparent node that gives way to its child passes the child's field up, so the
        // item lands in one field or the other.
        assert_eq!(
            named("outer"),
            [("k".to_owned(), Optional), ("w".to_owned(), Optional)]
        );
        // The node stays only for two names, which hold no field.
        assert_eq!(named("two"), []);
    }

    #[test]
    fn a_field_holds_exactly_one_item_when_every_match_fills_it() {
        let grammar = Grammar::from_text(
            "both: a=NAME | a=NUM\neither: a=NAME | NUM\nwrap: _wrap\n\
             _wrap: \"(\" _wrap \")\" | a=NAME\nloop: _loop\n_loop: \"(\" _loop | NUM\n\
             NAME: [a-z]+\nNUM: [0-9]+",
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let named = |kind| fields(&grammar, kind);
        assert_eq!(named("both"), [("a".to_owned(), One)]);
        assert_eq!(named("either"), [("a".to_owned(), Optional)]);
        // Every match of `_wrap` ends in a name, however deep, and `_loop` never has one.
        assert_eq!(named("wrap"), [("a".to_owned(), One)]);
        assert_eq!(named("loop"), []);
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
