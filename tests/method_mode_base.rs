//! Extending a visitor that is in heterogeneous (method-generic) mode.
//!
//! A base whose non-shared parameter carries a bound keys its trait on the shared subset and makes
//! the rest per-method generics, which also makes its methods `where Self: Sized`. An extender has
//! to match that: its forwarding impls must satisfy the base's as a supertrait, and a `?Sized`
//! visitor cannot.
#![allow(dead_code)]

use core::marker::PhantomData;
use visiting::Visiting;

pub trait Bound {}
impl Bound for () {}

#[derive(Visiting)]
pub struct Bounded<S>
where
    S: crate::Bound,
{
    pub p: PhantomData<S>,
}

#[derive(Visiting)]
pub struct Plain {
    pub n: u8,
}

#[derive(Visiting)]
#[visiting(crate::Bounded, crate::Plain)]
pub struct Top<S>
where
    S: crate::Bound,
{
    pub a: Bounded<S>,
    pub b: Plain,
}

// `S` is unshared (only `Bounded` has it) and bounded, so this goes method-mode.
pub mod base {
    visiting::define_visit!(crate::Bounded, crate::Plain);
}
pub mod ext {
    visiting::define_visit!(crate::base => crate::Top);
}

fn sample() -> Top<()> {
    Top {
        a: Bounded { p: PhantomData },
        b: Plain { n: 7 },
    }
}

#[derive(Default)]
struct Counter {
    bounded: u32,
    plain: u32,
    top: u32,
}

impl base::Visit for Counter {
    fn visit_bounded<S: crate::Bound>(&mut self, i: &Bounded<S>) {
        self.bounded += 1;
        base::visit_bounded(self, i);
    }
    fn visit_plain(&mut self, i: &Plain) {
        self.plain += 1;
        base::visit_plain(self, i);
    }
}
impl base::VisitMut for Counter {}
impl<S: crate::Bound> ext::Visit<S> for Counter {
    fn visit_top(&mut self, i: &Top<S>) {
        self.top += 1;
        ext::visit_top(self, i);
    }
}

#[test]
fn an_extended_method_mode_base_walks() {
    let mut c = Counter::default();
    ext::Visit::visit_top(&mut c, &sample());
    assert_eq!((c.bounded, c.plain, c.top), (1, 1, 1));
}

#[test]
fn a_method_mode_visitor_can_be_passed_by_reference() {
    let mut c = Counter::default();
    base::Visit::visit_plain(&mut &mut c, &Plain { n: 1 });
    assert_eq!(c.plain, 1);
}

#[test]
fn a_method_mode_visitor_can_be_boxed() {
    let mut b: Box<Counter> = Box::default();
    base::Visit::visit_plain(&mut *b, &Plain { n: 1 });
    assert_eq!(b.plain, 1);

    let mut boxed: Box<Counter> = Box::default();
    base::Visit::visit_plain(&mut boxed, &Plain { n: 1 });
    assert_eq!(boxed.plain, 1);
}

// The extender itself is not in method mode, so its own closure machinery is intact.
#[test]
fn a_closure_still_works_on_the_extender() {
    let mut tops = 0;
    sample().visit(|_t: &Top<()>| tops += 1);
    assert_eq!(tops, 1);
}

/// The chain keeps working past one level. `Self: Sized` travels the whole way (each link re-exports
/// it), and the ancestor impls state the predicates their traits were keyed on — which is what the
/// commit below this one added, and what this depth needs.
mod three_levels {
    use core::marker::PhantomData;
    use visiting::Visiting;

    #[derive(Visiting)]
    pub struct Tip<S>
    where
        S: crate::Bound,
    {
        pub p: PhantomData<S>,
    }

    #[derive(Visiting)]
    pub struct Flat {
        pub n: u8,
    }

    #[derive(Visiting)]
    #[visiting(crate::three_levels::Tip, crate::three_levels::Flat)]
    pub struct Mid<S>
    where
        S: crate::Bound,
    {
        pub a: Tip<S>,
        pub b: Flat,
    }

    #[derive(Visiting)]
    #[visiting(crate::three_levels::Mid)]
    pub struct Peak<S>
    where
        S: crate::Bound,
    {
        pub m: Mid<S>,
    }

    // `S` is unshared between `Tip` and `Flat`, and bounded, so `base` is heterogeneous.
    pub mod base {
        visiting::define_visit!(crate::three_levels::Tip, crate::three_levels::Flat);
    }
    pub mod mid {
        visiting::define_visit!(crate::three_levels::base => crate::three_levels::Mid);
    }
    pub mod top {
        visiting::define_visit!(crate::three_levels::mid => crate::three_levels::Peak);
    }

    #[derive(Default)]
    struct Counter {
        tips: u32,
        flats: u32,
        mids: u32,
        peaks: u32,
    }

    impl base::Visit for Counter {
        fn visit_tip<S: crate::Bound>(&mut self, i: &Tip<S>) {
            self.tips += 1;
            base::visit_tip(self, i);
        }
        fn visit_flat(&mut self, i: &Flat) {
            self.flats += 1;
            base::visit_flat(self, i);
        }
    }
    impl base::VisitMut for Counter {}
    impl<S: crate::Bound> mid::Visit<S> for Counter {
        fn visit_mid(&mut self, i: &Mid<S>) {
            self.mids += 1;
            mid::visit_mid(self, i);
        }
    }
    impl<S: crate::Bound> mid::VisitMut<S> for Counter {}
    impl<S: crate::Bound> top::Visit<S> for Counter {
        fn visit_peak(&mut self, i: &Peak<S>) {
            self.peaks += 1;
            top::visit_peak(self, i);
        }
    }

    #[test]
    fn a_heterogeneous_base_two_levels_down() {
        let p: Peak<()> = Peak {
            m: Mid {
                a: Tip { p: PhantomData },
                b: Flat { n: 1 },
            },
        };
        let mut c = Counter::default();
        top::Visit::visit_peak(&mut c, &p);
        assert_eq!((c.tips, c.flats, c.mids, c.peaks), (1, 1, 1, 1));
    }
}
