//! A method name no ancestor declares is rejected rather than dropped in silence: every muncher
//! would skip it, so the override would simply never run.

use ast::Ast;
use core::marker::PhantomData;

#[derive(Ast)]
pub enum Type<S> {
    Leaf(PhantomData<S>),
}

#[derive(Ast)]
#[subast(crate::Type)]
pub struct Item<S> {
    pub ty: Type<S>,
}

pub mod base {
    ast::visitor!(crate::Type);
}
pub mod top {
    ast::visitor!(crate::base => crate::Item);
}

pub struct P;

top::__ast_chain::impl_chain! { top; P;
    fn visit_nonsense(&mut self, i) { let _ = i; }
}

fn main() {}
