//! A method name no ancestor declares is rejected rather than dropped in silence: every muncher
//! would skip it, so the override would simply never run.

use visiting::Visiting;
use core::marker::PhantomData;

#[derive(Visiting)]
pub enum Type<S> {
    Leaf(PhantomData<S>),
}

#[derive(Visiting)]
#[visiting(crate::Type)]
pub struct Item<S> {
    pub ty: Type<S>,
}

pub mod base {
    visiting::define_visit!(crate::Type);
}
pub mod top {
    visiting::define_visit!(crate::base => crate::Item);
}

pub struct P;

top::__visiting_chain::impl_chain! { top; P;
    fn visit_nonsense(&mut self, i) { let _ = i; }
}

fn main() {}
