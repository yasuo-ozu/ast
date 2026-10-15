//! A base in heterogeneous mode declares a `visit_*` that takes its own type parameters.
//! `impl_chain!` cannot write such a method — nothing it generates can be `for<T>` generic — and
//! accepting it would make every muncher drop it, so the override would never run. The message has
//! to say that, not "no ancestor declares it": one does.

use core::marker::PhantomData;
use visiting::Visiting;

pub trait Bound {}

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

// `S` is unshared and bounded, so `visit_bounded` carries it as a method generic.
pub mod base {
    visiting::define_visit!(crate::Bounded, crate::Plain);
}
pub mod ext {
    visiting::define_visit!(crate::base => crate::Top);
}

pub struct P;

ext::__visiting_chain::impl_chain! { ext; P;
    fn visit_bounded(&mut self, i) { let _ = i; }
}

fn main() {}
