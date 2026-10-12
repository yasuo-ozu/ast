//! A five-level chain, and a chain whose levels differ in arity.
//!
//! The per-ancestor hook traits chain, so one bound reaches the whole line; these pin that it keeps
//! working at depth, and when the extender carries parameters its ancestors never had.
#![allow(dead_code)]

use core::marker::PhantomData;
use visiting::Visiting;

mod deep {
    use super::*;

    #[derive(Visiting)]
    pub enum A<S> {
        X(PhantomData<S>),
    }
    #[derive(Visiting)]
    #[subast(crate::deep::A)]
    pub struct B<S> {
        pub a: A<S>,
    }
    #[derive(Visiting)]
    #[subast(crate::deep::B)]
    pub struct C<S> {
        pub b: B<S>,
    }
    #[derive(Visiting)]
    #[subast(crate::deep::C)]
    pub struct D<S> {
        pub c: C<S>,
    }
    #[derive(Visiting)]
    #[subast(crate::deep::D)]
    pub struct E<S> {
        pub d: D<S>,
    }

    pub mod v1 {
        visiting::define_visit!(crate::deep::A);
    }
    pub mod v2 {
        visiting::define_visit!(crate::deep::v1 => crate::deep::B);
    }
    pub mod v3 {
        visiting::define_visit!(crate::deep::v2 => crate::deep::C);
    }
    pub mod v4 {
        visiting::define_visit!(crate::deep::v3 => crate::deep::D);
    }
    pub mod v5 {
        visiting::define_visit!(crate::deep::v4 => crate::deep::E);
    }

    fn sample() -> E<()> {
        E {
            d: D {
                c: C {
                    b: B {
                        a: A::X(PhantomData),
                    },
                },
            },
        }
    }

    #[test]
    fn closure_reaches_four_levels_up() {
        let mut n = 0;
        sample().visit(|_: &A<()>| n += 1);
        assert_eq!(n, 1);
    }

    #[test]
    fn tuple_spans_all_five_levels() {
        let (mut a, mut b, mut c, mut d, mut e) = (0, 0, 0, 0, 0);
        sample().visit((
            |_: &A<()>| a += 1,
            |_: &B<()>| b += 1,
            |_: &C<()>| c += 1,
            |_: &D<()>| d += 1,
            |_: &E<()>| e += 1,
        ));
        assert_eq!((a, b, c, d, e), (1, 1, 1, 1, 1));
    }

    #[derive(Default)]
    struct P {
        a: u32,
        e: u32,
    }
    v5::__visiting_chain::impl_chain! { v5; P;
        fn visit_a(&mut self, i) { self.a += 1; v1::visit_a(self, i); }
    }
    impl<S> v5::Visit<S> for P {
        fn visit_e(&mut self, i: &E<S>) {
            self.e += 1;
            v5::visit_e(self, i);
        }
    }

    #[test]
    fn a_struct_pass_reaches_four_levels_up() {
        let mut p = P::default();
        sample().visit(&mut p);
        assert_eq!((p.a, p.e), (1, 1));
    }
}

mod widened {
    use super::*;

    #[derive(Visiting)]
    pub enum Type<S> {
        U(PhantomData<S>),
    }
    #[derive(Visiting)]
    #[subast(crate::widened::Type)]
    pub struct Expr<S> {
        pub t: Type<S>,
    }
    // The extender carries a parameter its ancestors never had.
    #[derive(Visiting)]
    #[subast(crate::widened::Expr)]
    pub struct Item<S, T> {
        pub e: Expr<S>,
        pub tag: PhantomData<T>,
    }

    pub mod base {
        visiting::define_visit!(crate::widened::Type, crate::widened::Expr);
    }
    pub mod top {
        visiting::define_visit!(crate::widened::base => crate::widened::Item);
    }

    fn sample() -> Item<(), u8> {
        Item {
            e: Expr {
                t: Type::U(PhantomData),
            },
            tag: PhantomData,
        }
    }

    #[test]
    fn closure_over_a_narrower_ancestor() {
        let mut n = 0;
        sample().visit(|_: &Type<()>| n += 1);
        assert_eq!(n, 1);
    }

    #[test]
    fn tuple_spanning_both_arities() {
        let (mut a, mut b) = (0, 0);
        sample().visit((|_: &Item<(), u8>| a += 1, |_: &Type<()>| b += 1));
        assert_eq!((a, b), (1, 1));
    }

    #[derive(Default)]
    struct P {
        types: u32,
        items: u32,
    }
    top::__visiting_chain::impl_chain! { top; P;
        fn visit_type(&mut self, i) { self.types += 1; base::visit_type(self, i); }
    }
    impl<S, T> top::Visit<S, T> for P {
        fn visit_item(&mut self, i: &Item<S, T>) {
            self.items += 1;
            top::visit_item(self, i);
        }
    }

    #[test]
    fn a_struct_pass_across_arities() {
        let mut p = P::default();
        sample().visit(&mut p);
        assert_eq!((p.items, p.types), (1, 1));
    }
}
