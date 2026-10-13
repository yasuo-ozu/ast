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

use visiting_foreign_fixture::{inner::Branch, Aliased, Leaf, Marked, Tree};

pub mod visit {
    visiting::define_visit!(
        visiting_foreign_fixture::Tree,
        visiting_foreign_fixture::inner::Branch,
        visiting_foreign_fixture::Aliased,
        visiting_foreign_fixture::Marked,
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
    assert_eq!(
        seen,
        vec![5],
        "the field spelled `Twin` was followed as `Leaf`"
    );
}

/// `#[seq]`, `#[opt]` and `#[skip]` inside an announced definition. The markers are what make the
/// walk offer the *slot* — the whole `Vec`, the whole `Option` — instead of each element, and
/// `#[skip]` takes a field out of the walk. Until announce let a check own attribute names, none of
/// the three could be written on a type that does not derive, so an announced node was always
/// walked the plain way.
#[derive(Default)]
struct Slots {
    seq_len: usize,
    opt_present: bool,
    elements: Vec<u32>,
}

impl visit::Visit for Slots {
    fn visit_leaf_seq<'a, V: visiting::SeqView<'a, Leaf>>(&mut self, v: &'a V) {
        self.seq_len = v.len();
        self.elements.extend(v.elements().map(|l| l.n));
    }
    fn visit_leaf_opt<'a, V: visiting::OptView<'a, Leaf>>(&mut self, v: &'a V) {
        self.opt_present = v.is_some();
    }
}
impl visit::VisitMut for Slots {}

fn marked() -> Marked {
    Marked {
        many: vec![Leaf { n: 1 }, Leaf { n: 2 }],
        maybe: Some(Leaf { n: 3 }),
        hidden: Leaf { n: 99 },
    }
}

#[test]
fn the_field_markers_of_an_announced_node_reach_the_visitor() {
    use visit::Visit;
    let mut s = Slots::default();
    s.visit_marked(&marked());
    assert_eq!((s.seq_len, s.opt_present), (2, true));
    assert_eq!(
        s.elements,
        vec![1, 2],
        "the slot was offered, not each element"
    );
}

#[test]
fn a_skipped_field_of_an_announced_node_is_not_walked() {
    use visit::{IntoVisitor, Visit};
    let m = marked();
    let mut seen = Vec::new();
    {
        let mut v = (|l: &Leaf| seen.push(l.n)).into_visitor();
        v.visit_marked(&m);
    }
    // 1, 2 from the `#[seq]` field and 3 from the `#[opt]` one; 99 is behind `#[skip]`.
    seen.sort_unstable();
    assert_eq!(
        seen,
        vec![1, 2, 3],
        "`#[skip]` kept `hidden` out of the walk"
    );
}

#[test]
fn an_announced_node_can_write_its_follow_list_as_an_attribute() {
    // `Marked` lists `crate::Leaf` in `#[visiting(..)]` inside the definition rather than in the
    // check's arguments, and the walk above found the leaves through it.
    fn is_node<T: visiting::visit::Visiting>() {}
    is_node::<Marked>();
}

/// The editing half of a marker, on an announced node: the `&mut` walk is handed the slot, so it can
/// change the collection rather than only its elements.
#[derive(Default)]
struct Edit;

impl visit::Visit for Edit {}
impl visit::VisitMut for Edit {
    fn visit_leaf_seq_mut<'a, V: visiting::SeqEdit<'a, Leaf>>(&mut self, v: &'a mut V) {
        v.retain_mut(|l| l.n != 1);
        v.push(Leaf { n: 7 });
    }
    fn visit_leaf_opt_mut<'a, V: visiting::OptEdit<'a, Leaf>>(&mut self, v: &'a mut V) {
        v.take();
    }
}

#[test]
fn the_edit_views_of_an_announced_node_change_its_collections() {
    use visit::VisitMut;
    let mut m = marked();
    Edit.visit_marked_mut(&mut m);
    assert_eq!(
        m.many.iter().map(|l| l.n).collect::<Vec<_>>(),
        vec![2, 7],
        "the `#[seq]` slot was edited, not just read"
    );
    assert!(m.maybe.is_none(), "the `#[opt]` slot was cleared");
    assert_eq!(m.hidden.n, 99, "`#[skip]` is still skipped on the mut side");
}
