//! `#[seq]` and `#[opt]` end to end: the generated `visit_*_seq`/`visit_*_opt` methods, on both
//! sides.
//!
//! Without a marker the walk unwraps the container and offers each element to `visit_*`. With one it
//! offers the *slot* instead, which is the only way to see an element's index or its neighbours, and
//! on the `&mut` side to change the collection.
#![allow(dead_code)]

use ast::{Ast, OptView, SeqView};

#[derive(Ast, Debug, PartialEq)]
pub enum Leaf {
    N(u32),
}

#[derive(Ast, Debug)]
pub struct Holder {
    #[seq]
    pub items: Vec<Leaf>,
    #[opt]
    pub maybe: Option<Leaf>,
}

pub mod visit {
    ast::visitor!(crate::Leaf, crate::Holder);
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
    fn visit_leaf_seq<V: SeqView<Leaf>>(&mut self, v: &V) {
        self.len = v.len();
        for i in 0..v.len() {
            if matches!(&v[i], Leaf::N(n) if n % 2 == 1) {
                self.indices.push(i);
            }
        }
    }
    fn visit_leaf_opt<V: OptView<Leaf>>(&mut self, v: &V) {
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
    fn visit_leaf_seq_mut<V: SeqView<Leaf>>(&mut self, v: &mut V) {
        v.retain_mut(|e| !matches!(e, Leaf::N(2)));
        v.insert(0, Leaf::N(0));
    }
    fn visit_leaf_opt_mut<V: OptView<Leaf>>(&mut self, v: &mut V) {
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
