//! `#[seq]` and `#[opt]` end to end: the generated `visit_*_seq`/`visit_*_opt` methods, on both
//! sides.
//!
//! Without a marker the walk unwraps the container and offers each element to `visit_*`. With one it
//! offers the *slot* instead, which is the only way to see an element's index or its neighbours, and
//! on the `&mut` side to change the collection.
#![allow(dead_code)]

use visiting::{OptEdit, OptView, SeqEdit, SeqView, Visiting};

#[derive(Visiting, Debug, PartialEq)]
pub enum Leaf {
    N(u32),
}

#[derive(Visiting, Debug)]
pub struct Holder {
    #[seq]
    pub items: Vec<Leaf>,
    #[opt]
    pub maybe: Option<Leaf>,
}

pub mod visit {
    visiting::define_visit!(crate::Leaf, crate::Holder);
}

fn holder() -> Holder {
    Holder {
        items: vec![Leaf::N(1), Leaf::N(2), Leaf::N(3)],
        maybe: Some(Leaf::N(9)),
    }
}

// ---- the shared side sees the slot, not just an element ----

#[derive(Default)]
struct Observe {
    len: usize,
    indices: Vec<usize>,
    opt_present: bool,
}
impl visit::Visit for Observe {
    fn visit_leaf_seq<'a, V: SeqView<'a, Leaf>>(&mut self, v: &'a V) {
        self.len = v.len();
        for i in 0..v.len() {
            if matches!(&v[i], Leaf::N(n) if n % 2 == 1) {
                self.indices.push(i);
            }
        }
        // the same walk through the container's own iterator, which the `Elements` half of the
        // bound makes reachable from inside the override.
        let odd: Vec<usize> = v
            .elements()
            .enumerate()
            .filter(|(_, e)| matches!(e, Leaf::N(n) if n % 2 == 1))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(odd, self.indices);
    }
    fn visit_leaf_opt<'a, V: OptView<'a, Leaf>>(&mut self, v: &'a V) {
        self.opt_present = v.is_some();
    }
}
impl visit::VisitMut for Observe {}

#[test]
fn the_seq_view_reports_length_and_index() {
    let mut o = Observe::default();
    holder().visit(&mut o);
    assert_eq!(o.len, 3);
    assert_eq!(o.indices, vec![0, 2], "the odd values sit at 0 and 2");
}

#[test]
fn the_opt_view_reports_presence() {
    let mut o = Observe::default();
    holder().visit(&mut o);
    assert!(o.opt_present);

    let mut empty = Observe::default();
    Holder {
        items: vec![],
        maybe: None,
    }
    .visit(&mut empty);
    assert!(!empty.opt_present);
}

// ---- the mut side edits the slot in place ----

struct Edit;
impl visit::Visit for Edit {}
impl visit::VisitMut for Edit {
    fn visit_leaf_seq_mut<'a, V: SeqEdit<'a, Leaf>>(&mut self, v: &'a mut V) {
        v.retain_mut(|e| !matches!(e, Leaf::N(2)));
        v.insert(0, Leaf::N(0));
    }
    fn visit_leaf_opt_mut<'a, V: OptEdit<'a, Leaf>>(&mut self, v: &'a mut V) {
        v.take();
    }
}

#[test]
fn the_mut_views_change_the_collection() {
    let mut h = holder();
    h.visit_mut(&mut Edit);
    assert_eq!(h.items, vec![Leaf::N(0), Leaf::N(1), Leaf::N(3)]);
    assert_eq!(h.maybe, None);
}

// ---- the default body still descends ----

#[derive(Default)]
struct CountLeaves(u32);
impl visit::Visit for CountLeaves {
    fn visit_leaf(&mut self, _i: &Leaf) {
        self.0 += 1;
    }
}
impl visit::VisitMut for CountLeaves {}

#[test]
fn a_marked_field_is_still_walked_element_by_element() {
    let mut c = CountLeaves::default();
    holder().visit(&mut c);
    assert_eq!(c.0, 4, "three in the Vec, one in the Option");
}

// ---- a node that is not `'static` ----
//
// A view's bound is `SeqView<'vw, T>`, whose `Elements<'vw, T>` supertrait declares `T: 'vw`. That
// predicate is not inferred, so the generated method carries it as a `where`-clause; without it the
// whole trait fails to compile the moment a visited type holds a borrow. This module is what pins
// that: `Borrowed<'s>` outlives nothing in particular, and both sides still iterate it.
mod borrowed_elements {
    use super::*;

    #[derive(Visiting, Debug, PartialEq)]
    pub struct Borrowed<'s>(pub &'s str);

    #[derive(Visiting, Debug)]
    #[subast(crate::borrowed_elements::Borrowed)]
    pub struct Doc<'s> {
        #[seq]
        pub words: Vec<Borrowed<'s>>,
    }

    pub mod v {
        visiting::define_visit!(
            crate::borrowed_elements::Borrowed,
            crate::borrowed_elements::Doc
        );
    }

    #[derive(Default)]
    struct Collect(Vec<String>);
    impl<'s> v::Visit<'s> for Collect {
        fn visit_borrowed_seq<'a, V: SeqView<'a, Borrowed<'s>>>(&mut self, v: &'a V)
        where
            Borrowed<'s>: 'a,
        {
            self.0 = v.elements().map(|b| b.0.to_string()).collect();
        }
    }
    impl<'s> v::VisitMut<'s> for Collect {}

    struct Upper;
    impl<'s> v::Visit<'s> for Upper {}
    impl<'s> v::VisitMut<'s> for Upper {
        fn visit_borrowed_seq_mut<'a, V: SeqEdit<'a, Borrowed<'s>>>(&mut self, v: &'a mut V)
        where
            Borrowed<'s>: 'a,
        {
            for b in v.elements_mut() {
                b.0 = "x";
            }
        }
    }

    #[test]
    fn a_borrowed_element_iterates_on_both_sides() {
        // owned here, so the element type is genuinely not `'static`.
        let owned = String::from("hello");
        let mut d = Doc {
            words: vec![Borrowed(&owned), Borrowed("world")],
        };

        let mut c = Collect::default();
        d.visit(&mut c);
        assert_eq!(c.0, vec!["hello".to_string(), "world".to_string()]);

        d.visit_mut(&mut Upper);
        assert_eq!(d.words, vec![Borrowed("x"), Borrowed("x")]);
    }
}
