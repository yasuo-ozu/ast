//! A cycle of *unlisted* intermediates (`A -> B -> A`, neither in `define_visit!(..)`) cannot be drilled
//! inline — it would expand forever. `__visitor_build` must reject it, pointing the user at listing
//! one of the cycle's types so a method call breaks the recursion.

use visiting::Visiting;

#[derive(Visiting)]
#[visiting(crate::B)]
pub struct A<S>(pub ::std::boxed::Box<crate::B<S>>, pub ::core::marker::PhantomData<S>);

#[derive(Visiting)]
#[visiting(crate::A)]
pub struct B<S>(pub ::std::boxed::Box<crate::A<S>>, pub ::core::marker::PhantomData<S>);

#[derive(Visiting)]
#[visiting(crate::A)]
pub enum Expr<S> {
    Wrap(crate::A<S>),
    Lit(::core::marker::PhantomData<S>),
}

pub mod visit {
    visiting::define_visit!(super::Expr);
}

fn main() {}
