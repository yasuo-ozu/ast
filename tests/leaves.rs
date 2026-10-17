//! The field-level markers and the crate-path override: `#[skip]`, and `#[visiting(crate = <path>)]` for a
//! grammar that reaches this crate through a facade.
#![allow(dead_code)]

// `#[skip]` prunes one field. The walk still enters its siblings, so this is not "stop here".
mod skip {

    #[announce::announce(visiting::visiting)]
    pub struct Leaf(pub u32);

    #[announce::announce(visiting::visiting)]
    #[visiting(crate::skip::Leaf)]
    pub struct Node {
        pub a: crate::skip::Leaf,
        #[skip]
        pub b: crate::skip::Leaf,
        pub c: ::std::vec::Vec<crate::skip::Leaf>,
    }

    pub mod v {
        visiting::define_visit!(super::Leaf, super::Node);
    }

    #[test]
    fn a_skipped_field_is_not_entered() {
        let n = Node {
            a: Leaf(1),
            b: Leaf(2),
            c: vec![Leaf(3)],
        };
        let mut seen = Vec::new();
        n.visit(|l: &Leaf| seen.push(l.0));
        assert_eq!(seen, vec![1, 3], "`b` is skipped, `a` and `c` are not");
    }

    #[test]
    fn a_skipped_field_is_not_entered_mutably() {
        let mut n = Node {
            a: Leaf(1),
            b: Leaf(2),
            c: vec![Leaf(3)],
        };
        n.visit_mut(|l: &mut Leaf| l.0 *= 10);
        assert_eq!((n.a.0, n.b.0, n.c[0].0), (10, 2, 30));
    }
}

// `#[visiting(crate = <path>)]` makes the check name a facade instead of `::visiting`, so a grammar depending only
// on that facade needs no direct dependency on this crate. The module below stands in for one: the
// check is reached through it too, and so is `define_visit!`.
mod facade {
    pub mod my_crate {
        pub use visiting::*;
    }

    #[announce::announce(crate::facade::my_crate::visiting)]
    #[visiting(crate = crate::facade::my_crate)]
    pub struct Leaf(pub u32);

    #[announce::announce(crate::facade::my_crate::visiting)]
    #[visiting(crate = crate::facade::my_crate)]
    #[visiting(crate::facade::Leaf)]
    pub struct Node {
        pub leaves: ::std::vec::Vec<crate::facade::Leaf>,
    }

    pub mod v {
        crate::facade::my_crate::define_visit!(super::Leaf, super::Node);
    }

    #[test]
    fn the_check_names_the_facade() {
        let n = Node {
            leaves: vec![Leaf(1), Leaf(2)],
        };
        let mut sum = 0;
        n.visit(|l: &Leaf| sum += l.0);
        assert_eq!(sum, 3);
    }
}
