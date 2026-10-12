//! A cycle of *unlisted* intermediates (`A -> B -> A`, neither in `define_visit!(..)`) cannot be drilled
//! inline — it would expand forever. `__visitor_build` must reject it, pointing the user at listing
//! one of the cycle's types so a method call breaks the recursion.

use core::marker::PhantomData;
use visiting::Visiting;

#[derive(Visiting)]
#[subast(crate::B)]
pub struct A<S>(pub Box<B<S>>, pub PhantomData<S>);

#[derive(Visiting)]
#[subast(crate::A)]
pub struct B<S>(pub Box<A<S>>, pub PhantomData<S>);

#[derive(Visiting)]
#[subast(crate::A)]
pub enum Expr<S> {
    Wrap(A<S>),
    Lit(PhantomData<S>),
}

pub mod visit {
    visiting::define_visit!(super::Expr);
}

fn main() {}
