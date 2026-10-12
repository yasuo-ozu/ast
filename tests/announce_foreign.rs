//! A visitor over types from a crate that only announces.
//!
//! This is the case the `visiting(..)` check exists for. `visiting-foreign-fixture` defines the
//! nodes; it derives nothing of ours and writes no impls. The marker `impl Visiting` cannot be
//! written *here* — the trait and the types are both foreign, so the orphan rule forbids it — and
//! that is what used to stop a foreign announced type from being walked at all. The check puts the
//! impl beside the definition instead, and the follow-list the fixture wrote as `crate::Leaf` has to
//! arrive still naming the fixture.
//!
//! A foreign type cannot carry the inherent `visit` method either (E0116), so the walk starts
//! through the generated trait: `Visit::visit_tree(&mut visitor, &tree)`, with a closure turned into
//! a visitor by `into_visitor`.
#![allow(dead_code)]

use visiting_foreign_fixture::{inner::Branch, Aliased, Leaf, Tree};

pub mod visit {
    visiting::define_visit!(
        visiting_foreign_fixture::Tree,
        visiting_foreign_fixture::inner::Branch,
        visiting_foreign_fixture::Aliased,
        visiting_foreign_fixture::Leaf
    );
}

#[test]
fn a_foreign_announced_tree_is_walked() {
    use visit::{IntoVisitor, Visit};
    let t = Tree {
        head: Leaf { n: 1 },
        rest: vec![Leaf { n: 2 }, Leaf { n: 3 }],
    };
    let mut seen = Vec::new();
    {
        let mut v = (|l: &Leaf| seen.push(l.n)).into_visitor();
        v.visit_tree(&t);
    }
    assert_eq!(seen, vec![1, 2, 3]);
}

#[test]
fn a_foreign_node_in_a_submodule_is_walked() {
    use visit::{IntoVisitor, Visit};
    let b = Branch {
        leaf: Leaf { n: 4 },
    };
    let mut seen = Vec::new();
    {
        let mut v = (|l: &Leaf| seen.push(l.n)).into_visitor();
        v.visit_branch(&b);
    }
    assert_eq!(seen, vec![4]);
}

#[test]
fn the_foreign_types_are_nodes_without_this_crate_implementing_anything() {
    fn is_node<T: visiting::visit::Visiting>() {}
    is_node::<Leaf>();
    is_node::<Tree>();
    is_node::<Branch>();
}

#[test]
fn an_edit_walk_reaches_them_too() {
    use visit::{IntoVisitorMut, VisitMut};
    let mut t = Tree {
        head: Leaf { n: 1 },
        rest: vec![Leaf { n: 2 }],
    };
    let mut v = (|l: &mut Leaf| l.n *= 10).into_visitor_mut();
    v.visit_tree_mut(&mut t);
    assert_eq!((t.head.n, t.rest[0].n), (10, 20));
}

#[test]
fn an_aliased_follow_entry_is_matched() {
    use visit::{IntoVisitor, Visit};
    let a = Aliased {
        twin: Leaf { n: 5 },
    };
    let mut seen = Vec::new();
    {
        let mut v = (|l: &Leaf| seen.push(l.n)).into_visitor();
        v.visit_aliased(&a);
    }
    assert_eq!(seen, vec![5], "the field spelled `Twin` was followed as `Leaf`");
}
