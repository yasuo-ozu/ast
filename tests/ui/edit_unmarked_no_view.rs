//! Container-edit views (`visit_<t>_seq` / `visit_<t>_opt`) are generated ONLY for a field explicitly
//! marked `#[seq]` / `#[opt]` — there is no auto-detection from the container type. Here `Leaf` is held
//! in a `Vec` and an `Option`, but neither field is marked, so no view method exists (only the in-place
//! `visit_leaf_mut`). Overriding `visit_leaf_seq_mut` / `visit_leaf_opt_mut` is a "not a member of trait" error.

use core::marker::PhantomData;
use ast::{Ast, OptView, SeqView};

#[derive(Ast)]
pub enum Leaf<S> {
    A(PhantomData<S>),
}

#[derive(Ast)]
#[subast(crate::Leaf)]
pub struct Wrap<S> {
    pub items: Vec<Leaf<S>>,   // not `#[seq]` -> no `visit_leaf_seq_mut`
    pub last: Option<Leaf<S>>, // not `#[opt]` -> no `visit_leaf_opt_mut`
}

pub mod vis {
    ast::visitor!(crate::Wrap, crate::Leaf);
}

struct V;
impl<S> vis::VisitMut<S> for V {
    fn visit_leaf_seq_mut<'v, W: SeqView<'v, Leaf<S>>>(&mut self, _v: &'v mut W)
    where
        S: 'v,
    {
    }
    fn visit_leaf_opt_mut<W: OptView<Leaf<S>>>(&mut self, _v: &mut W) {}
}

fn main() {
    let _ = PhantomData::<()>;
}
