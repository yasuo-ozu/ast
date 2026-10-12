//! Container/tuple traversal shapes on ordinary (non-recurse) ASTs: nested containers,
//! container-of-tuple, tuple fields.
#![allow(dead_code)]

// Nested containers (`Vec<Option<T>>`, `Option<Vec<T>>`, `Vec<Vec<T>>`) are traversed.
mod nested {
    use core::marker::PhantomData;
    use visiting::Visiting;

    #[derive(Visiting)]
    #[subast()]
    pub struct Leaf<S> {
        pub _p: PhantomData<S>,
        pub value: i64,
    }

    #[derive(Visiting)]
    #[subast(crate::nested::Leaf)]
    pub struct Holder<S> {
        pub vo: Vec<Option<Leaf<S>>>,
        pub ov: Option<Vec<Leaf<S>>>,
        pub vv: Vec<Vec<Leaf<S>>>,
    }

    mod v {
        visiting::define_visit!(crate::nested::Leaf, crate::nested::Holder);
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
    use core::marker::PhantomData;
    use visiting::Visiting;

    #[derive(Visiting)]
    #[subast()]
    pub struct Leaf<S> {
        pub _p: PhantomData<S>,
    }

    #[derive(Visiting)]
    #[subast(crate::container_of_tuple::Leaf)]
    pub struct Holder<S> {
        pub top: (Leaf<S>, Leaf<S>),
        pub vec_of_tuples: Vec<(Leaf<S>, Leaf<S>)>,
        pub opt_of_tuple: Option<(Leaf<S>, Leaf<S>)>,
        pub boxed_tuple: Box<(Leaf<S>, Leaf<S>)>,
    }

    mod v {
        visiting::define_visit!(
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
    use core::marker::PhantomData;
    use visiting::Visiting;

    #[derive(Visiting)]
    pub enum Ty<S> {
        Unit(PhantomData<S>),
    }

    #[derive(Visiting)]
    #[subast(crate::tuple_field::Ty)]
    pub enum Expr<S> {
        Pair((Ty<S>, Ty<S>)),
        // Nested tuple with a leaf element exercises recursion + `_` binding for non-followed members.
        Triple((Ty<S>, (PhantomData<S>, Ty<S>))),
        Lit(PhantomData<S>),
    }

    pub mod v {
        visiting::define_visit!(crate::tuple_field::Expr, crate::tuple_field::Ty);
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

// `View` is the hand-written-traversal half for a map field: the node sits in the VALUE slot, so
// there is no positional edit and nothing `define_visit!` generates calls it. Its two iterators are the
// map's own `Values`/`ValuesMut` — named as associated types, with the borrow a parameter of the
// trait, so neither is boxed.
mod map_view {
    use std::collections::{BTreeMap, HashMap};
    use visiting::Elements;

    #[derive(Debug, PartialEq)]
    struct Node(i64);

    fn pairs() -> [(&'static str, Node); 3] {
        [("a", Node(1)), ("b", Node(2)), ("c", Node(3))]
    }

    #[test]
    fn btree_map_reads_and_edits_its_values() {
        let mut m: BTreeMap<&str, Node> = pairs().into_iter().collect();

        let mut seen: Vec<i64> = m.elements().map(|n| n.0).collect();
        seen.sort_unstable();
        assert_eq!(seen, vec![1, 2, 3]);

        for n in m.elements_mut() {
            n.0 *= 10;
        }
        assert_eq!(m["a"], Node(10));
        assert_eq!(m["c"], Node(30));
    }

    #[test]
    fn hash_map_reads_and_edits_its_values() {
        let mut m: HashMap<&str, Node> = pairs().into_iter().collect();

        let mut seen: Vec<i64> = m.elements().map(|n| n.0).collect();
        seen.sort_unstable();
        assert_eq!(seen, vec![1, 2, 3]);

        for n in m.elements_mut() {
            n.0 += 1;
        }
        let mut seen: Vec<i64> = m.elements().map(|n| n.0).collect();
        seen.sort_unstable();
        assert_eq!(seen, vec![2, 3, 4]);
    }

    // A caller generic over the trait: the iterators come back as associated types, so this needs
    // no `Box` and no `dyn`.
    fn total<'a, M: Elements<'a, Node>>(m: &'a M) -> i64 {
        m.elements().map(|n| n.0).sum()
    }

    #[test]
    fn a_generic_caller_names_the_iterator_as_an_associated_type() {
        let m: BTreeMap<&str, Node> = pairs().into_iter().collect();
        assert_eq!(total(&m), 6);
    }
}

// Containers the crate has never heard of. `#[seq]`/`#[opt]` match no container by name — they accept
// any field that peels to exactly one layer — so a consumer plugs its own in with three impls:
// `SeqView`/`OptView` for the edit view, and `Walk`/`WalkMut` at `Thru<H>` for the descent.
mod own_container {
    use visiting::walk::{Thru, Walk, WalkMut};
    use visiting::{Elements, OptEdit, OptView, SeqEdit, SeqView, Visiting};

    pub struct Punctuated<T, P>(Vec<T>, core::marker::PhantomData<P>);

    impl<T, P> Punctuated<T, P> {
        fn new(items: Vec<T>) -> Self {
            Punctuated(items, core::marker::PhantomData)
        }
    }

    // `SeqView`'s element access is its `Index`/`IndexMut` supertraits, so a consumer container
    // supplies those and the trait itself only has to say how long the slot is and how to resize it.
    impl<T, P> core::ops::Index<usize> for Punctuated<T, P> {
        type Output = T;
        fn index(&self, i: usize) -> &T {
            &self.0[i]
        }
    }
    impl<T, P> core::ops::IndexMut<usize> for Punctuated<T, P> {
        fn index_mut(&mut self, i: usize) -> &mut T {
            &mut self.0[i]
        }
    }

    impl<'a, T: 'a, P> SeqView<'a, T> for Punctuated<T, P> {
        fn len(&self) -> usize {
            self.0.len()
        }
    }

    // Resizable, so it gets the edit half too — which is what a `#[seq]` field's `&mut` side asks for.
    impl<'a, T: 'a, P: Default> SeqEdit<'a, T> for Punctuated<T, P> {
        fn insert(&mut self, i: usize, value: T) {
            self.0.insert(i, value)
        }
        fn remove(&mut self, i: usize) -> T {
            self.0.remove(i)
        }
    }

    // Optional, and separate: the edit view above is all `#[seq]` needs. This is for a visitor that
    // wants to iterate the slot itself, and it hands back the container's own iterators.
    impl<'a, T: 'a, P> Elements<'a, T> for Punctuated<T, P> {
        type Iter = core::slice::Iter<'a, T>;
        type IterMut = core::slice::IterMut<'a, T>;
        fn elements(&'a self) -> Self::Iter {
            self.0.iter()
        }
        fn elements_mut(&'a mut self) -> Self::IterMut {
            self.0.iter_mut()
        }
    }

    /// A second container of the consumer's own, this one `Option`-shaped: `#[opt]` takes it through
    /// `OptView` exactly as it takes `Option<T>`.
    pub struct Optional<T>(Option<T>);

    impl<T> Optional<T> {
        fn new(value: Option<T>) -> Self {
            Optional(value)
        }
    }

    // `OptView`'s supertrait is `Elements`, so a consumer's `Option`-shaped container has to iterate
    // as well — even though an opt slot holds at most one node and the view reaches it through
    // `get`/`get_mut`. `Option`'s own iterators do the job.
    impl<'a, T: 'a> Elements<'a, T> for Optional<T> {
        type Iter = core::option::Iter<'a, T>;
        type IterMut = core::option::IterMut<'a, T>;
        fn elements(&'a self) -> Self::Iter {
            self.0.iter()
        }
        fn elements_mut(&'a mut self) -> Self::IterMut {
            self.0.iter_mut()
        }
    }

    impl<'a, T: 'a> OptView<'a, T> for Optional<T> {
        fn is_some(&self) -> bool {
            self.0.is_some()
        }
        fn get(&self) -> Option<&T> {
            self.0.as_ref()
        }
    }

    impl<'a, T: 'a> OptEdit<'a, T> for Optional<T> {
        fn get_mut(&mut self) -> Option<&mut T> {
            self.0.as_mut()
        }
        fn set(&mut self, value: T) {
            self.0 = Some(value);
        }
        fn take(&mut self) -> Option<T> {
            self.0.take()
        }
    }

    impl<M, H, V: ?Sized, T: Walk<M, H, V>> Walk<M, Thru<H>, V> for Optional<T> {
        fn walk(&self, v: &mut V) {
            if let Some(x) = &self.0 {
                x.walk(v);
            }
        }
    }
    impl<M, H, V: ?Sized, T: WalkMut<M, H, V>> WalkMut<M, Thru<H>, V> for Optional<T> {
        fn walk_mut(&mut self, v: &mut V) {
            if let Some(x) = &mut self.0 {
                x.walk_mut(v);
            }
        }
    }

    impl<M, H, V: ?Sized, T: Walk<M, H, V>, P> Walk<M, Thru<H>, V> for Punctuated<T, P> {
        fn walk(&self, v: &mut V) {
            for x in &self.0 {
                x.walk(v);
            }
        }
    }
    impl<M, H, V: ?Sized, T: WalkMut<M, H, V>, P> WalkMut<M, Thru<H>, V> for Punctuated<T, P> {
        fn walk_mut(&mut self, v: &mut V) {
            for x in &mut self.0 {
                x.walk_mut(v);
            }
        }
    }

    #[derive(Visiting)]
    pub struct Item(pub u32);

    #[derive(Visiting)]
    #[subast(crate::own_container::Item)]
    pub struct Doc {
        #[seq]
        pub items: Punctuated<Item, char>,
        #[opt]
        pub trailing: Optional<Item>,
    }

    pub mod v {
        visiting::define_visit!(super::Item, super::Doc);
    }

    #[test]
    fn a_consumer_container_is_walked() {
        let d = Doc {
            items: Punctuated::new(vec![Item(1), Item(2), Item(3)]),
            trailing: Optional::new(Some(Item(4))),
        };
        let mut seen = Vec::new();
        d.visit(|i: &Item| seen.push(i.0));
        assert_eq!(seen, vec![1, 2, 3, 4], "both consumer containers descended");
    }

    #[test]
    fn a_consumer_container_takes_a_seq_edit() {
        struct DropOdd;
        impl v::VisitMut for DropOdd {
            fn visit_item_seq_mut<'a, S: SeqEdit<'a, Item>>(&mut self, s: &'a mut S) {
                s.retain_mut(|i| i.0 % 2 == 0);
            }
            fn visit_item_opt_mut<'a, O: OptEdit<'a, Item>>(&mut self, o: &'a mut O) {
                o.take();
            }
        }

        let mut d = Doc {
            items: Punctuated::new(vec![Item(1), Item(2), Item(3), Item(4)]),
            trailing: Optional::new(Some(Item(9))),
        };
        d.visit_mut(&mut DropOdd);
        let mut left = Vec::new();
        d.visit(|i: &Item| left.push(i.0));
        assert_eq!(
            left,
            vec![2, 4],
            "both views edited containers `ast` never names"
        );
    }

    // The shared side of both views: the slot itself, read through the consumer's own impls.
    #[test]
    fn a_consumer_container_is_read_through_its_view() {
        #[derive(Default)]
        struct Shape {
            len: usize,
            iterated: Vec<u32>,
            trailing: bool,
        }
        impl v::Visit for Shape {
            // The seq view's bound names `Elements` too, so the consumer's *own* iterator —
            // `Punctuated::iter`, here — is reachable from inside the override, next to `len` and
            // the `Index` supertrait.
            fn visit_item_seq<'a, S: SeqView<'a, Item>>(&mut self, s: &'a S) {
                self.len = s.len();
                self.iterated = s.elements().map(|i| i.0).collect();
            }
            fn visit_item_opt<'a, O: OptView<'a, Item>>(&mut self, o: &'a O) {
                self.trailing = o.is_some();
            }
        }

        let d = Doc {
            items: Punctuated::new(vec![Item(1), Item(2), Item(3)]),
            trailing: Optional::new(None),
        };
        let mut shape = Shape::default();
        d.visit(&mut shape);
        assert_eq!(shape.len, 3);
        assert_eq!(
            shape.iterated,
            vec![1, 2, 3],
            "through `Punctuated`'s own iterator"
        );
        assert!(!shape.trailing);
    }
}

// A map the crate has never heard of. `View` is descent-only — the node sits in the VALUE slot, so
// there is no `#[seq]`/`#[opt]` marker and nothing `define_visit!` generates calls the trait. A consumer
// still gets both halves: `Walk`/`WalkMut` at `Thru<H>` for the generated descent, and `MapView` for
// a traversal it writes itself.
mod own_map {
    use visiting::walk::{Thru, Walk, WalkMut};
    use visiting::{Elements, Visiting};

    /// An association list, which is a map whose values are *projected* out of its entries rather
    /// than stored apart. So its iterators are `Map<slice::Iter<..>, fn(..) -> ..>` — nameable
    /// because the projection is a function item, not a closure. That is the point of the borrow
    /// being a trait parameter: a projecting map still needs no `Box<dyn Iterator>`.
    pub struct Registry<K, V>(Vec<(K, V)>);

    impl<K, V> Registry<K, V> {
        fn new(entries: Vec<(K, V)>) -> Self {
            Registry(entries)
        }
    }

    fn value<K, V>(e: &(K, V)) -> &V {
        &e.1
    }
    fn value_mut<K, V>(e: &mut (K, V)) -> &mut V {
        &mut e.1
    }

    impl<'a, K: 'a, V: 'a> Elements<'a, V> for Registry<K, V> {
        type Iter = core::iter::Map<core::slice::Iter<'a, (K, V)>, fn(&'a (K, V)) -> &'a V>;
        type IterMut =
            core::iter::Map<core::slice::IterMut<'a, (K, V)>, fn(&'a mut (K, V)) -> &'a mut V>;
        fn elements(&'a self) -> Self::Iter {
            self.0.iter().map(value as fn(&'a (K, V)) -> &'a V)
        }
        fn elements_mut(&'a mut self) -> Self::IterMut {
            self.0
                .iter_mut()
                .map(value_mut as fn(&'a mut (K, V)) -> &'a mut V)
        }
    }

    impl<M, H, Vis: ?Sized, K, T: Walk<M, H, Vis>> Walk<M, Thru<H>, Vis> for Registry<K, T> {
        fn walk(&self, v: &mut Vis) {
            for (_, t) in &self.0 {
                t.walk(v);
            }
        }
    }
    impl<M, H, Vis: ?Sized, K, T: WalkMut<M, H, Vis>> WalkMut<M, Thru<H>, Vis> for Registry<K, T> {
        fn walk_mut(&mut self, v: &mut Vis) {
            for (_, t) in &mut self.0 {
                t.walk_mut(v);
            }
        }
    }

    #[derive(Visiting)]
    pub struct Item(pub u32);

    #[derive(Visiting)]
    #[subast(crate::own_map::Item)]
    pub struct Doc {
        // No marker: a keyed slot has no positional edit, so the field is walked and nothing else.
        pub table: Registry<&'static str, Item>,
    }

    pub mod v {
        visiting::define_visit!(super::Item, super::Doc);
    }

    fn doc() -> Doc {
        Doc {
            table: Registry::new(vec![("a", Item(1)), ("b", Item(2)), ("c", Item(3))]),
        }
    }

    #[test]
    fn a_consumer_map_is_walked() {
        let mut seen = Vec::new();
        doc().visit(|i: &Item| seen.push(i.0));
        assert_eq!(seen, vec![1, 2, 3], "descended into the value slot");
    }

    #[test]
    fn a_consumer_map_is_walked_mutably() {
        let mut d = doc();
        d.visit_mut(|i: &mut Item| i.0 *= 10);
        let mut seen = Vec::new();
        d.visit(|i: &Item| seen.push(i.0));
        assert_eq!(seen, vec![10, 20, 30]);
    }

    #[test]
    fn the_map_view_reads_and_edits_the_values() {
        let mut r = doc().table;
        assert_eq!(r.elements().map(|i| i.0).collect::<Vec<_>>(), vec![1, 2, 3]);
        for i in r.elements_mut() {
            i.0 += 100;
        }
        assert_eq!(
            r.elements().map(|i| i.0).collect::<Vec<_>>(),
            vec![101, 102, 103]
        );
    }

    // The same generic caller the std maps get: the iterator arrives as an associated type, so this
    // needs no `Box` and no `dyn` even though `Registry` projects its values out of a pair.
    fn total<'a, M: Elements<'a, Item>>(m: &'a M) -> u32 {
        m.elements().map(|i| i.0).sum()
    }

    #[test]
    fn a_generic_caller_names_the_projected_iterator() {
        assert_eq!(total(&doc().table), 6);
    }
}

// `Vec` is the container every other test reaches for. These are the walk targets that nothing
// else exercises: a `VecDeque`, a fixed-size array and a slice behind a reference.
mod other_shapes {
    use std::collections::VecDeque;
    use visiting::Visiting;

    #[derive(Visiting)]
    pub struct Leaf(pub u32);

    #[derive(Visiting)]
    #[subast(crate::other_shapes::Leaf)]
    pub struct Holder {
        pub deque: VecDeque<Leaf>,
        pub array: [Leaf; 3],
    }

    pub mod v {
        visiting::define_visit!(super::Leaf, super::Holder);
    }

    fn sample() -> Holder {
        Holder {
            deque: VecDeque::from(vec![Leaf(1), Leaf(2)]),
            array: [Leaf(3), Leaf(4), Leaf(5)],
        }
    }

    #[test]
    fn a_deque_and_an_array_are_both_walked() {
        let mut seen = Vec::new();
        sample().visit(|l: &Leaf| seen.push(l.0));
        assert_eq!(seen, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_deque_and_an_array_are_walked_mutably() {
        let mut h = sample();
        h.visit_mut(|l: &mut Leaf| l.0 *= 2);
        let mut seen = Vec::new();
        h.visit(|l: &Leaf| seen.push(l.0));
        assert_eq!(seen, vec![2, 4, 6, 8, 10]);
    }
}
