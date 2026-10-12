//! Two different paths ending in the same identifier, inside one definition. A visitor recognises a
//! field's head by its LAST path segment, so `Stmt` and `super::other::Stmt` are indistinguishable
//! there — whichever is the visited type, the other is matched too. `#[derive(Visiting)]` rejects it at
//! the definition that carries the ambiguity.
//!
//! This is the shape behind the old `bug7` regression, where the generated visitor mis-called
//! `visit_stmt` on the foreign type. It is now refused rather than handled.
use core::marker::PhantomData;
use visiting::Visiting;

mod other {
    pub struct Stmt;
}

#[derive(Visiting)]
#[subast()]
pub enum Expr<S> {
    ToStmt(Box<Stmt<S>>),
    Foreign(crate::other::Stmt),
    Lit(PhantomData<S>),
}

#[derive(Visiting)]
#[subast()]
pub enum Stmt<S> {
    Back(Box<Expr<S>>),
    Nop(PhantomData<S>),
}

fn main() {}
