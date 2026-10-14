//! Extending a visitor whose visited type carries a `where` clause.
//!
//! A node's predicates ride into its visitor's trait, so `base::Visit<S>` can require `S: Bound`.
//! Every reference to that trait has to state the bound, including the empty impls the extender's
//! `Driver` writes to satisfy its supertraits.
#![allow(dead_code)]

use core::marker::PhantomData;
use visiting::Visiting;

pub trait Bound {}
impl Bound for () {}

#[derive(Visiting)]
pub struct Leaf<S>
where
    S: crate::Bound,
{
    pub p: PhantomData<S>,
}

#[derive(Visiting)]
#[visiting(crate::Leaf)]
pub struct Node<S>
where
    S: crate::Bound,
{
    pub a: Leaf<S>,
}

#[derive(Visiting)]
#[visiting(crate::Node)]
pub struct Root<S>
where
    S: crate::Bound,
{
    pub n: Node<S>,
}

pub mod base {
    visiting::define_visit!(crate::Leaf);
}
pub mod mid {
    visiting::define_visit!(crate::base => crate::Node);
}
// Two levels deep, so the `Driver` writes an impl for an ancestor that is not its direct base.
pub mod top {
    visiting::define_visit!(crate::mid => crate::Root);
}

fn sample() -> Root<()> {
    Root {
        n: Node {
            a: Leaf { p: PhantomData },
        },
    }
}

#[derive(Default)]
struct Counter {
    leaves: u32,
    nodes: u32,
    roots: u32,
}

impl<S: crate::Bound> base::Visit<S> for Counter {
    fn visit_leaf(&mut self, i: &Leaf<S>) {
        self.leaves += 1;
        base::visit_leaf(self, i);
    }
}
impl<S: crate::Bound> base::VisitMut<S> for Counter {}
impl<S: crate::Bound> mid::Visit<S> for Counter {
    fn visit_node(&mut self, i: &Node<S>) {
        self.nodes += 1;
        mid::visit_node(self, i);
    }
}
impl<S: crate::Bound> mid::VisitMut<S> for Counter {}
impl<S: crate::Bound> top::Visit<S> for Counter {
    fn visit_root(&mut self, i: &Root<S>) {
        self.roots += 1;
        top::visit_root(self, i);
    }
}

#[test]
fn a_bounded_param_survives_two_levels_of_inheritance() {
    let mut c = Counter::default();
    sample().visit(&mut c);
    assert_eq!((c.leaves, c.nodes, c.roots), (1, 1, 1));
}

#[test]
fn a_closure_still_works_over_the_bounded_grammar() {
    let mut n = 0;
    sample().visit(|_: &Root<()>| n += 1);
    assert_eq!(n, 1);
}

/// The other half of the rule: a predicate the ancestor's trait was *not* keyed on must stay off its
/// items. The extender adds a second, independently bounded parameter the base does not have; those
/// items are generic only in the ancestor's own parameters, so stating it there is `E0425: cannot
/// find type T in this scope`.
mod an_extra_param_the_base_does_not_have {
    use core::marker::PhantomData;
    use visiting::Visiting;

    pub trait Other {}
    impl Other for u8 {}

    #[derive(Visiting)]
    pub struct Tip<S>
    where
        S: crate::Bound,
    {
        pub p: PhantomData<S>,
    }

    #[derive(Visiting)]
    #[visiting(crate::an_extra_param_the_base_does_not_have::Tip)]
    pub struct Pair<S, T>
    where
        S: crate::Bound,
        T: crate::an_extra_param_the_base_does_not_have::Other,
    {
        pub a: Tip<S>,
        pub t: PhantomData<T>,
    }

    pub mod base {
        visiting::define_visit!(crate::an_extra_param_the_base_does_not_have::Tip);
    }
    pub mod top {
        visiting::define_visit!(crate::an_extra_param_the_base_does_not_have::base
            => crate::an_extra_param_the_base_does_not_have::Pair);
    }

    #[derive(Default)]
    struct Counter {
        tips: u32,
        pairs: u32,
    }

    impl<S: crate::Bound> base::Visit<S> for Counter {
        fn visit_tip(&mut self, i: &Tip<S>) {
            self.tips += 1;
            base::visit_tip(self, i);
        }
    }
    impl<S: crate::Bound> base::VisitMut<S> for Counter {}
    impl<S: crate::Bound, T: Other> top::Visit<S, T> for Counter {
        fn visit_pair(&mut self, i: &Pair<S, T>) {
            self.pairs += 1;
            top::visit_pair(self, i);
        }
    }

    fn sample() -> Pair<(), u8> {
        Pair {
            a: Tip { p: PhantomData },
            t: PhantomData,
        }
    }

    #[test]
    fn the_extra_predicate_stays_off_the_ancestor_impls() {
        let mut c = Counter::default();
        top::Visit::visit_pair(&mut c, &sample());
        assert_eq!((c.tips, c.pairs), (1, 1));
    }

    #[test]
    fn a_closure_reaches_the_inherited_node() {
        let mut n = 0;
        sample().visit(|_: &Tip<()>| n += 1);
        assert_eq!(n, 1);
    }
}
