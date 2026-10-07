//! The field-level markers and the crate-path override: `#[skip]`, and `#[ast(<path>)]` for a
//! grammar that reaches this crate through a facade.
#![allow(dead_code)]

// `#[skip]` prunes one field. The walk still enters its siblings, so this is not "stop here".
mod skip {
    use ast::Ast;

    #[derive(Ast)]
    pub struct Leaf(pub u32);

    #[derive(Ast)]
    #[subast(crate::skip::Leaf)]
    pub struct Node {
        pub a: Leaf,
        #[skip]
        pub b: Leaf,
        pub c: Vec<Leaf>,
    }

    pub mod v {
        ast::visitor!(super::Leaf, super::Node);
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

// `#[ast(<path>)]` makes the derive name a facade instead of `::ast`, so a grammar depending only
// on that facade needs no direct dependency on this crate. The module below stands in for one.
mod facade {
    pub mod my_crate {
        pub use ast::*;
    }

    use my_crate::Ast;

    #[derive(Ast)]
    #[ast(crate::facade::my_crate)]
    pub struct Leaf(pub u32);

    #[derive(Ast)]
    #[ast(crate::facade::my_crate)]
    #[subast(crate::facade::Leaf)]
    pub struct Node {
        pub leaves: Vec<Leaf>,
    }

    pub mod v {
        ast::visitor!(super::Leaf, super::Node);
    }

    #[test]
    fn the_derive_names_the_facade() {
        let n = Node {
            leaves: vec![Leaf(1), Leaf(2)],
        };
        let mut sum = 0;
        n.visit(|l: &Leaf| sum += l.0);
        assert_eq!(sum, 3);
    }
}
