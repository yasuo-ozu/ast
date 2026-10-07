//! Container/tuple traversal shapes on ordinary (non-recurse) ASTs: nested containers,
//! container-of-tuple, tuple fields.
#![allow(dead_code)]

// Nested containers (`Vec<Option<T>>`, `Option<Vec<T>>`, `Vec<Vec<T>>`) are traversed.
mod nested {
    use ast::Ast;
    use core::marker::PhantomData;

    #[derive(Ast)]
    #[subast()]
    pub struct Leaf<S> {
        pub _p: PhantomData<S>,
        pub value: i64,
    }

    #[derive(Ast)]
    #[subast(crate::nested::Leaf)]
    pub struct Holder<S> {
        pub vo: Vec<Option<Leaf<S>>>,
        pub ov: Option<Vec<Leaf<S>>>,
        pub vv: Vec<Vec<Leaf<S>>>,
    }

    mod v {
        ast::visitor!(crate::nested::Leaf, crate::nested::Holder);
    }

    fn leaf<S>(value: i64) -> Leaf<S> {
        Leaf {
            _p: PhantomData,
            value,
        }
    }

    #[test]
    fn nested_containers_are_traversed() {
        let h: Holder<()> = Holder {
            vo: vec![Some(leaf(1)), None, Some(leaf(2))],
            ov: Some(vec![leaf(3), leaf(4)]),
            vv: vec![vec![leaf(5)], vec![leaf(6), leaf(7)]],
        };
        let mut n = 0usize;
        h.visit(|_: &Leaf<()>| n += 1);
        assert_eq!(n, 7, "2 (Vec<Option>) + 2 (Option<Vec>) + 3 (Vec<Vec>)");
    }

    #[test]
    fn nested_containers_visit_mut() {
        let mut h: Holder<()> = Holder {
            vo: vec![Some(leaf(1))],
            ov: None,
            vv: vec![vec![leaf(2)]],
        };
        let mut n = 0usize;
        h.visit_mut(|l: &mut Leaf<()>| {
            n += 1;
            l.value = -l.value;
        });
        assert_eq!(
            n, 2,
            "Vec<Option<_>> and Vec<Vec<_>> both descended on the mut side"
        );
        // The mutation must persist through the owning container, not just be observed transiently by
        // the visitor closure.
        assert_eq!(
            h.vo[0].as_ref().map(|l| l.value),
            Some(-1),
            "Vec<Option<Leaf>> write persists"
        );
        assert_eq!(h.vv[0][0].value, -2, "Vec<Vec<Leaf>> write persists");
    }
}

// A tuple nested inside a container (`Vec<(A,B)>`, `Option<(A,B)>`, `Box<(A,B)>`) has its elements visited.
mod container_of_tuple {
    use ast::Ast;
    use core::marker::PhantomData;

    #[derive(Ast)]
    #[subast()]
    pub struct Leaf<S> {
        pub _p: PhantomData<S>,
    }

    #[derive(Ast)]
    #[subast(crate::container_of_tuple::Leaf)]
    pub struct Holder<S> {
        pub top: (Leaf<S>, Leaf<S>),
        pub vec_of_tuples: Vec<(Leaf<S>, Leaf<S>)>,
        pub opt_of_tuple: Option<(Leaf<S>, Leaf<S>)>,
        pub boxed_tuple: Box<(Leaf<S>, Leaf<S>)>,
    }

    mod v {
        ast::visitor!(
            crate::container_of_tuple::Leaf,
            crate::container_of_tuple::Holder
        );
    }

    fn leaf<S>() -> Leaf<S> {
        Leaf { _p: PhantomData }
    }

    #[test]
    fn container_of_tuple_visits_elements() {
        let h: Holder<()> = Holder {
            top: (leaf(), leaf()),
            vec_of_tuples: vec![(leaf(), leaf()), (leaf(), leaf())],
            opt_of_tuple: Some((leaf(), leaf())),
            boxed_tuple: Box::new((leaf(), leaf())),
        };
        let mut n = 0usize;
        h.visit(|_: &Leaf<()>| n += 1);
        assert_eq!(n, 10, "a container-of-tuple must visit its tuple elements");
    }

    #[test]
    fn container_of_tuple_visits_elements_mut() {
        let mut h: Holder<()> = Holder {
            top: (leaf(), leaf()),
            vec_of_tuples: vec![(leaf(), leaf())],
            opt_of_tuple: Some((leaf(), leaf())),
            boxed_tuple: Box::new((leaf(), leaf())),
        };
        let mut n = 0usize;
        h.visit_mut(|_: &mut Leaf<()>| n += 1);
        assert_eq!(
            n, 8,
            "the &mut side must also reach tuple elements inside containers"
        );
    }
}

// A tuple-typed field is destructured and each element lowered (top-level + nested tuples).
mod tuple_field {
    use ast::Ast;
    use core::marker::PhantomData;

    #[derive(Ast)]
    pub enum Ty<S> {
        Unit(PhantomData<S>),
    }

    #[derive(Ast)]
    #[subast(crate::tuple_field::Ty)]
    pub enum Expr<S> {
        Pair((Ty<S>, Ty<S>)),
        // Nested tuple with a leaf element exercises recursion + `_` binding for non-followed members.
        Triple((Ty<S>, (PhantomData<S>, Ty<S>))),
        Lit(PhantomData<S>),
    }

    pub mod v {
        ast::visitor!(crate::tuple_field::Expr, crate::tuple_field::Ty);
    }

    #[test]
    fn tuple_field_visits_each_element() {
        let e = Expr::Pair((Ty::Unit(PhantomData), Ty::Unit(PhantomData)));
        let mut n = 0usize;
        e.visit(|_: &Ty<()>| n += 1);
        assert_eq!(n, 2, "both tuple elements should be visited");
    }

    #[test]
    fn nested_tuple_with_leaf_element() {
        let e = Expr::Triple((Ty::Unit(PhantomData), (PhantomData, Ty::Unit(PhantomData))));
        let mut n = 0usize;
        e.visit(|_: &Ty<()>| n += 1);
        assert_eq!(
            n, 2,
            "the two Ty elements (skipping the PhantomData leaf) should be visited"
        );
    }
}

// `MapView` is the hand-written-traversal view for a map field: the node sits in the VALUE slot, so
// there is no positional edit and nothing `visitor!` generates calls it. Its two iterators are the
// map's own `Values`/`ValuesMut` — named as associated types, with the borrow a parameter of the
// trait, so neither is boxed.
mod map_view {
    use ast::MapView;
    use std::collections::{BTreeMap, HashMap};

    #[derive(Debug, PartialEq)]
    struct Node(i64);

    fn pairs() -> [(&'static str, Node); 3] {
        [("a", Node(1)), ("b", Node(2)), ("c", Node(3))]
    }

    #[test]
    fn btree_map_reads_and_edits_its_values() {
        let mut m: BTreeMap<&str, Node> = pairs().into_iter().collect();

        let mut seen: Vec<i64> = m.view_iter().map(|n| n.0).collect();
        seen.sort_unstable();
        assert_eq!(seen, vec![1, 2, 3]);

        for n in m.view_iter_mut() {
            n.0 *= 10;
        }
        assert_eq!(m["a"], Node(10));
        assert_eq!(m["c"], Node(30));
    }

    #[test]
    fn hash_map_reads_and_edits_its_values() {
        let mut m: HashMap<&str, Node> = pairs().into_iter().collect();

        let mut seen: Vec<i64> = m.view_iter().map(|n| n.0).collect();
        seen.sort_unstable();
        assert_eq!(seen, vec![1, 2, 3]);

        for n in m.view_iter_mut() {
            n.0 += 1;
        }
        let mut seen: Vec<i64> = m.view_iter().map(|n| n.0).collect();
        seen.sort_unstable();
        assert_eq!(seen, vec![2, 3, 4]);
    }

    // A caller generic over the trait: the iterators come back as associated types, so this needs
    // no `Box` and no `dyn`.
    fn total<'a, M: MapView<'a, Node>>(m: &'a M) -> i64 {
        m.view_iter().map(|n| n.0).sum()
    }

    #[test]
    fn a_generic_caller_names_the_iterator_as_an_associated_type() {
        let m: BTreeMap<&str, Node> = pairs().into_iter().collect();
        assert_eq!(total(&m), 6);
    }
}
