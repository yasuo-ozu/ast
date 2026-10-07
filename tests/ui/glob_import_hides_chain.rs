//! A glob import of a visitor module brings in what a user writes by hand — `Visit`, `VisitMut`,
//! the free `visit_*` functions, `IntoVisitor` — and none of the chain machinery. `impl_chain!`
//! lives one module down, so the glob takes the module's name and not its contents.

use core::marker::PhantomData;
use ast::Ast;

#[derive(Ast)]
pub enum Item<S> {
    Leaf(PhantomData<S>),
}

#[derive(Ast)]
#[subast(crate::Item)]
pub struct List<S> {
    pub items: Vec<Item<S>>,
}

pub mod base {
    ast::visitor!(crate::Item);
}

pub mod top {
    ast::visitor!(crate::base => crate::List);
}

use top::*;

pub struct P;

// The glob did carry the trait, which this names without an import...
fn _named_through_the_glob<S, V: Visit<S>>(_: &V) {}

// ...and not this.
impl_chain!(top; P);

fn main() {}
