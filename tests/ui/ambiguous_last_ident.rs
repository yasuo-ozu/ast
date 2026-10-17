//! Two different paths ending in the same identifier, inside one definition. A visitor recognises a
//! field's head by its LAST path segment, so `Stmt` and `super::other::Stmt` are indistinguishable
//! there — whichever is the visited type, the other is matched too. The check rejects it at the
//! definition that carries the ambiguity.
//!
//! This is the shape behind the old `bug7` regression, where the generated visitor mis-called
//! `visit_stmt` on the foreign type. It is now refused rather than handled.

mod other {
    pub struct Stmt;
}

#[visiting::visiting]
#[visiting()]
pub enum Expr<S> {
    ToStmt(::std::boxed::Box<crate::Stmt<S>>),
    Foreign(crate::other::Stmt),
    Lit(::core::marker::PhantomData<S>),
}

#[visiting::visiting]
#[visiting()]
pub enum Stmt<S> {
    Back(::std::boxed::Box<crate::Expr<S>>),
    Nop(::core::marker::PhantomData<S>),
}

fn main() {}
