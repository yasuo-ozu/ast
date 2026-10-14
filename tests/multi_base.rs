//! `define_visit!(a, b => T)` — a visitor extending more than one other visitor.
#![allow(dead_code)]

use core::marker::PhantomData;
use visiting::Visiting;

#[derive(Visiting)]
pub enum Ty<S> {
    Unit(PhantomData<S>),
}

#[derive(Visiting)]
pub enum Lit<S> {
    Num(PhantomData<S>),
}

#[derive(Visiting)]
#[visiting(crate::Ty, crate::Lit)]
pub struct Expr<S> {
    pub t: Ty<S>,
    pub l: Lit<S>,
}

// Two independent visitors, neither extending the other.
pub mod types {
    visiting::define_visit!(crate::Ty);
}
pub mod lits {
    visiting::define_visit!(crate::Lit);
}

// One visitor extending both.
pub mod both {
    visiting::define_visit!(crate::types, crate::lits => crate::Expr);
}

#[derive(Default)]
struct Counter {
    tys: u32,
    lits: u32,
    exprs: u32,
}

impl<S> types::Visit<S> for Counter {
    fn visit_ty(&mut self, i: &Ty<S>) {
        self.tys += 1;
        types::visit_ty(self, i);
    }
}
impl<S> types::VisitMut<S> for Counter {}
impl<S> lits::Visit<S> for Counter {
    fn visit_lit(&mut self, i: &Lit<S>) {
        self.lits += 1;
        lits::visit_lit(self, i);
    }
}
impl<S> lits::VisitMut<S> for Counter {}
impl<S> both::Visit<S> for Counter {
    fn visit_expr(&mut self, i: &Expr<S>) {
        self.exprs += 1;
        both::visit_expr(self, i);
    }
}

fn sample() -> Expr<()> {
    Expr {
        t: Ty::Unit(PhantomData),
        l: Lit::Num(PhantomData),
    }
}

#[test]
fn both_bases_are_reached() {
    let mut c = Counter::default();
    sample().visit(&mut c);
    assert_eq!((c.tys, c.lits, c.exprs), (1, 1, 1));
}

// A closure covers this visitor's own types and the ones it inherits, from either base — the
// ancestor hooks answer for the latter, and there is now one family of them per base.
#[test]
fn a_closure_covers_its_own_type() {
    let mut n = 0;
    sample().visit(|_: &Expr<()>| n += 1);
    assert_eq!(n, 1);
}

#[test]
fn a_closure_covers_a_node_of_either_base() {
    let (mut t, mut l) = (0, 0);
    sample().visit(|_: &Ty<()>| t += 1);
    sample().visit(|_: &Lit<()>| l += 1);
    assert_eq!((t, l), (1, 1));
}

// A tuple folds its closures into one hook, so all of them run in a single walk — across both
// bases at once here, which needs each ancestor's 2-tuple impl as well as this visitor's.
#[test]
fn a_tuple_of_closures_spans_both_bases_in_one_walk() {
    let (mut e, mut t, mut l) = (0, 0, 0);
    sample().visit((
        |_: &Expr<()>| e += 1,
        |_: &Ty<()>| t += 1,
        |_: &Lit<()>| l += 1,
    ));
    assert_eq!((e, t, l), (1, 1, 1));
}

#[test]
fn a_tuple_of_closures_edits_across_both_bases() {
    let (mut t, mut l) = (0, 0);
    let mut e = sample();
    e.visit_mut((|_: &mut Ty<()>| t += 1, |_: &mut Lit<()>| l += 1));
    assert_eq!((t, l), (1, 1));
}

// Each base's own entry points keep working on a value that implements the whole set.
#[test]
fn each_base_can_be_driven_directly() {
    let mut c = Counter::default();
    let (t, l): (Ty<()>, Lit<()>) = (Ty::Unit(PhantomData), Lit::Num(PhantomData));
    types::Visit::visit_ty(&mut c, &t);
    lits::Visit::visit_lit(&mut c, &l);
    assert_eq!((c.tys, c.lits), (1, 1));
}

// Passing by `&mut` and boxed still resolves with several supertraits in play. (A tuple is a
// visitor only when every element is a closure, so two struct passes are two walks — see
// `composition.rs`.)
#[test]
fn wrappers_still_satisfy_every_supertrait() {
    let mut c = Counter::default();
    sample().visit(&mut c);
    assert_eq!(c.exprs, 1);

    let mut b: Box<Counter> = Box::default();
    both::Visit::visit_expr(&mut *b, &sample());
    assert_eq!(b.exprs, 1);

    let mut boxed: Box<Counter> = Box::default();
    both::Visit::visit_expr(&mut boxed, &sample());
    assert_eq!(boxed.exprs, 1);
}

// A diamond: both bases extend one shared visitor, so its trait is a supertrait twice over and the
// ancestor list must name it once.
mod diamond {
    use core::marker::PhantomData;
    use visiting::Visiting;

    #[derive(Visiting)]
    pub enum Leaf<S> {
        L(PhantomData<S>),
    }
    #[derive(Visiting)]
    #[visiting(crate::diamond::Leaf)]
    pub struct Left<S> {
        pub a: Leaf<S>,
    }
    #[derive(Visiting)]
    #[visiting(crate::diamond::Leaf)]
    pub struct Right<S> {
        pub a: Leaf<S>,
    }
    #[derive(Visiting)]
    #[visiting(crate::diamond::Left, crate::diamond::Right)]
    pub struct Top<S> {
        pub l: Left<S>,
        pub r: Right<S>,
    }

    pub mod shared {
        visiting::define_visit!(crate::diamond::Leaf);
    }
    pub mod l {
        visiting::define_visit!(crate::diamond::shared => crate::diamond::Left);
    }
    pub mod r {
        visiting::define_visit!(crate::diamond::shared => crate::diamond::Right);
    }
    pub mod top {
        visiting::define_visit!(crate::diamond::l, crate::diamond::r => crate::diamond::Top);
    }

    #[derive(Default)]
    struct C {
        leaves: u32,
        tops: u32,
    }
    impl<S> shared::Visit<S> for C {
        fn visit_leaf(&mut self, i: &Leaf<S>) {
            self.leaves += 1;
            shared::visit_leaf(self, i);
        }
    }
    impl<S> shared::VisitMut<S> for C {}
    impl<S> l::Visit<S> for C {}
    impl<S> l::VisitMut<S> for C {}
    impl<S> r::Visit<S> for C {}
    impl<S> r::VisitMut<S> for C {}
    impl<S> top::Visit<S> for C {
        fn visit_top(&mut self, i: &Top<S>) {
            self.tops += 1;
            top::visit_top(self, i);
        }
    }

    fn sample() -> Top<()> {
        Top {
            l: Left {
                a: Leaf::L(PhantomData),
            },
            r: Right {
                a: Leaf::L(PhantomData),
            },
        }
    }

    // The shared ancestor's node occurs under both bases, and a closure for it fires at each
    // occurrence — one hook family, reached down the single surviving relay path.
    #[test]
    fn a_closure_on_the_shared_ancestors_node_fires_at_each_occurrence() {
        let mut n = 0;
        sample().visit(|_: &Leaf<()>| n += 1);
        assert_eq!(n, 2);
    }

    #[test]
    fn a_tuple_of_closures_spans_the_diamond() {
        let (mut t, mut lf) = (0, 0);
        sample().visit((|_: &Top<()>| t += 1, |_: &Leaf<()>| lf += 1));
        assert_eq!((t, lf), (1, 2));
    }

    #[test]
    fn a_shared_ancestor_is_named_once() {
        let mut c = C::default();
        sample().visit(&mut c);
        assert_eq!((c.leaves, c.tops), (2, 1));
    }
}

// Bases of different arity, and a further visitor extending the multi-base one.
mod widened {
    use core::marker::PhantomData;
    use visiting::Visiting;

    #[derive(Visiting)]
    pub enum A<S> {
        X(PhantomData<S>),
    }
    #[derive(Visiting)]
    pub enum B<S, T> {
        Y(PhantomData<(S, T)>),
    }
    #[derive(Visiting)]
    #[visiting(crate::widened::A, crate::widened::B)]
    pub struct Both<S, T> {
        pub a: A<S>,
        pub b: B<S, T>,
    }
    #[derive(Visiting)]
    #[visiting(crate::widened::Both)]
    pub struct Outer<S, T> {
        pub inner: Both<S, T>,
    }

    pub mod va {
        visiting::define_visit!(crate::widened::A);
    }
    pub mod vb {
        visiting::define_visit!(crate::widened::B);
    }
    pub mod vboth {
        visiting::define_visit!(crate::widened::va, crate::widened::vb => crate::widened::Both);
    }
    pub mod vouter {
        visiting::define_visit!(crate::widened::vboth => crate::widened::Outer);
    }

    #[derive(Default)]
    struct C {
        a: u32,
        b: u32,
        outer: u32,
    }
    impl<S> va::Visit<S> for C {
        fn visit_a(&mut self, i: &A<S>) {
            self.a += 1;
            va::visit_a(self, i);
        }
    }
    impl<S> va::VisitMut<S> for C {}
    impl<S, T> vb::Visit<S, T> for C {
        fn visit_b(&mut self, i: &B<S, T>) {
            self.b += 1;
            vb::visit_b(self, i);
        }
    }
    impl<S, T> vb::VisitMut<S, T> for C {}
    impl<S, T> vboth::Visit<S, T> for C {}
    impl<S, T> vboth::VisitMut<S, T> for C {}
    impl<S, T> vouter::Visit<S, T> for C {
        fn visit_outer(&mut self, i: &Outer<S, T>) {
            self.outer += 1;
            vouter::visit_outer(self, i);
        }
    }

    #[test]
    fn bases_of_different_arity_and_a_further_extender() {
        let o: Outer<(), ()> = Outer {
            inner: Both {
                a: A::X(PhantomData),
                b: B::Y(PhantomData),
            },
        };
        let mut c = C::default();
        o.visit(&mut c);
        assert_eq!((c.a, c.b, c.outer), (1, 1, 1));
    }
}

// Three bases, and a closure for a node of each.
mod three {
    use visiting::Visiting;

    #[derive(Visiting)]
    pub struct A {
        pub n: u8,
    }
    #[derive(Visiting)]
    pub struct B {
        pub n: u8,
    }
    #[derive(Visiting)]
    pub struct C {
        pub n: u8,
    }
    #[derive(Visiting)]
    #[visiting(crate::three::A, crate::three::B, crate::three::C)]
    pub struct T {
        pub a: A,
        pub b: B,
        pub c: C,
    }

    pub mod va {
        visiting::define_visit!(crate::three::A);
    }
    pub mod vb {
        visiting::define_visit!(crate::three::B);
    }
    pub mod vc {
        visiting::define_visit!(crate::three::C);
    }
    pub mod vt {
        visiting::define_visit!(crate::three::va, crate::three::vb, crate::three::vc
            => crate::three::T);
    }

    #[derive(Default)]
    struct P {
        a: u32,
        b: u32,
        c: u32,
        t: u32,
    }
    impl va::Visit for P {
        fn visit_a(&mut self, i: &A) {
            self.a += 1;
            va::visit_a(self, i);
        }
    }
    impl va::VisitMut for P {}
    impl vb::Visit for P {
        fn visit_b(&mut self, i: &B) {
            self.b += 1;
            vb::visit_b(self, i);
        }
    }
    impl vb::VisitMut for P {}
    impl vc::Visit for P {
        fn visit_c(&mut self, i: &C) {
            self.c += 1;
            vc::visit_c(self, i);
        }
    }
    impl vc::VisitMut for P {}
    impl vt::Visit for P {
        fn visit_t(&mut self, i: &T) {
            self.t += 1;
            vt::visit_t(self, i);
        }
    }

    fn sample() -> T {
        T {
            a: A { n: 1 },
            b: B { n: 2 },
            c: C { n: 3 },
        }
    }

    #[test]
    fn every_base_is_reached() {
        let mut p = P::default();
        sample().visit(&mut p);
        assert_eq!((p.a, p.b, p.c, p.t), (1, 1, 1, 1));
    }

    #[test]
    fn a_closure_lands_on_a_node_of_each_base() {
        let (mut na, mut nb, mut nc) = (0, 0, 0);
        sample().visit(|_: &A| na += 1);
        sample().visit(|_: &B| nb += 1);
        sample().visit(|_: &C| nc += 1);
        assert_eq!((na, nb, nc), (1, 1, 1));
    }
}

/// A base listed *beside* a visitor that already extends it. Who-extends-whom then points backwards
/// along the chain, so reachability has to be a closure over the graph rather than an ordered walk.
mod base_and_its_extender {
    use visiting::Visiting;

    #[derive(Visiting)]
    pub struct Leaf {
        pub n: u8,
    }
    #[derive(Visiting)]
    #[visiting(crate::base_and_its_extender::Leaf)]
    pub struct Mid {
        pub l: Leaf,
    }
    #[derive(Visiting)]
    #[visiting(crate::base_and_its_extender::Leaf, crate::base_and_its_extender::Mid)]
    pub struct Top {
        pub l: Leaf,
        pub m: Mid,
    }

    pub mod low {
        visiting::define_visit!(crate::base_and_its_extender::Leaf);
    }
    pub mod mid {
        visiting::define_visit!(crate::base_and_its_extender::low
            => crate::base_and_its_extender::Mid);
    }
    pub mod top {
        visiting::define_visit!(crate::base_and_its_extender::low,
            crate::base_and_its_extender::mid => crate::base_and_its_extender::Top);
    }

    #[derive(Default)]
    struct P {
        leaf: u32,
        mid: u32,
        top: u32,
    }
    impl low::Visit for P {
        fn visit_leaf(&mut self, i: &Leaf) {
            self.leaf += 1;
            low::visit_leaf(self, i);
        }
    }
    impl low::VisitMut for P {}
    impl mid::Visit for P {
        fn visit_mid(&mut self, i: &Mid) {
            self.mid += 1;
            mid::visit_mid(self, i);
        }
    }
    impl mid::VisitMut for P {}
    impl top::Visit for P {
        fn visit_top(&mut self, i: &Top) {
            self.top += 1;
            top::visit_top(self, i);
        }
    }

    #[test]
    fn a_base_beside_its_own_extender() {
        let t = Top {
            l: Leaf { n: 1 },
            m: Mid { l: Leaf { n: 2 } },
        };
        let mut p = P::default();
        top::Visit::visit_top(&mut p, &t);
        assert_eq!((p.leaf, p.mid, p.top), (2, 1, 1));
    }
}

/// A multi-base visitor extended by a second one, with every ancestor impl written by `impl_chain!`.
mod stacked {
    use visiting::Visiting;

    #[derive(Visiting)]
    pub struct A {
        pub n: u8,
    }
    #[derive(Visiting)]
    pub struct B {
        pub n: u8,
    }
    #[derive(Visiting)]
    pub struct C {
        pub n: u8,
    }
    #[derive(Visiting)]
    #[visiting(crate::stacked::A, crate::stacked::B)]
    pub struct Ab {
        pub a: A,
        pub b: B,
    }
    #[derive(Visiting)]
    #[visiting(crate::stacked::Ab, crate::stacked::C)]
    pub struct Top {
        pub ab: Ab,
        pub c: C,
    }

    pub mod va {
        visiting::define_visit!(crate::stacked::A);
    }
    pub mod vb {
        visiting::define_visit!(crate::stacked::B);
    }
    pub mod vc {
        visiting::define_visit!(crate::stacked::C);
    }
    pub mod vab {
        visiting::define_visit!(crate::stacked::va, crate::stacked::vb => crate::stacked::Ab);
    }
    pub mod vtop {
        visiting::define_visit!(crate::stacked::vab, crate::stacked::vc => crate::stacked::Top);
    }

    #[derive(Default)]
    struct P {
        a: u32,
        b: u32,
        ab: u32,
        top: u32,
    }

    vtop::__visiting_chain::impl_chain! { vtop; P;
        fn visit_a(&mut self, i) { self.a += 1; let _ = i; }
        fn visit_b(&mut self, i) { self.b += 1; let _ = i; }
        fn visit_ab(&mut self, i) { self.ab += 1; vab::visit_ab(self, i); }
    }
    impl vtop::Visit for P {
        fn visit_top(&mut self, i: &Top) {
            self.top += 1;
            vtop::visit_top(self, i);
        }
    }

    fn sample() -> Top {
        Top {
            ab: Ab {
                a: A { n: 1 },
                b: B { n: 2 },
            },
            c: C { n: 3 },
        }
    }

    #[test]
    fn impl_chain_covers_a_multi_base_chain() {
        let mut p = P::default();
        vtop::Visit::visit_top(&mut p, &sample());
        assert_eq!((p.a, p.b, p.ab, p.top), (1, 1, 1, 1));
    }

    #[test]
    fn a_closure_reaches_a_grandparents_node() {
        let mut n = 0;
        sample().visit(|_: &A| n += 1);
        assert_eq!(n, 1);
    }
}

/// One base in heterogeneous mode beside one that is not: the `Self: Sized` the first demands has to
/// travel, and the second must not be dragged into method-generic shape.
mod one_heterogeneous_base {
    use core::marker::PhantomData;
    use visiting::Visiting;

    pub trait Bound {}
    impl Bound for () {}

    #[derive(Visiting)]
    pub struct Bounded<S>
    where
        S: crate::one_heterogeneous_base::Bound,
    {
        pub p: PhantomData<S>,
    }
    #[derive(Visiting)]
    pub struct Plain {
        pub n: u8,
    }
    #[derive(Visiting)]
    pub struct Other {
        pub n: u8,
    }
    #[derive(Visiting)]
    #[visiting(
        crate::one_heterogeneous_base::Bounded,
        crate::one_heterogeneous_base::Plain,
        crate::one_heterogeneous_base::Other
    )]
    pub struct Top<S>
    where
        S: crate::one_heterogeneous_base::Bound,
    {
        pub a: Bounded<S>,
        pub b: Plain,
        pub c: Other,
    }

    // `het` is heterogeneous (`S` is unshared and bounded); `plainv` is not.
    pub mod het {
        visiting::define_visit!(
            crate::one_heterogeneous_base::Bounded,
            crate::one_heterogeneous_base::Plain
        );
    }
    pub mod plainv {
        visiting::define_visit!(crate::one_heterogeneous_base::Other);
    }
    pub mod both {
        visiting::define_visit!(crate::one_heterogeneous_base::het,
            crate::one_heterogeneous_base::plainv => crate::one_heterogeneous_base::Top);
    }

    #[derive(Default)]
    struct P {
        bounded: u32,
        plain: u32,
        other: u32,
        top: u32,
    }
    impl het::Visit for P {
        fn visit_bounded<S: Bound>(&mut self, i: &Bounded<S>) {
            self.bounded += 1;
            het::visit_bounded(self, i);
        }
        fn visit_plain(&mut self, i: &Plain) {
            self.plain += 1;
            het::visit_plain(self, i);
        }
    }
    impl het::VisitMut for P {}
    impl plainv::Visit for P {
        fn visit_other(&mut self, i: &Other) {
            self.other += 1;
            plainv::visit_other(self, i);
        }
    }
    impl plainv::VisitMut for P {}
    impl<S: Bound> both::Visit<S> for P {
        fn visit_top(&mut self, i: &Top<S>) {
            self.top += 1;
            both::visit_top(self, i);
        }
    }

    #[test]
    fn a_heterogeneous_base_among_several() {
        let t: Top<()> = Top {
            a: Bounded { p: PhantomData },
            b: Plain { n: 1 },
            c: Other { n: 2 },
        };
        let mut p = P::default();
        both::Visit::visit_top(&mut p, &t);
        assert_eq!((p.bounded, p.plain, p.other, p.top), (1, 1, 1, 1));
    }
}
