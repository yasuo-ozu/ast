//! A `#[subast(..)]` entry has to be an AST node itself. Without the check the omission surfaces
//! only once a `visitor!` tries to expand the entry's metadata macro, as `cannot find `Plain` in
//! `$crate`` — a macro the user never wrote, reported far from the attribute at fault.

use ast::Ast;

pub struct Plain<S>(S);

#[derive(Ast)]
#[subast(crate::Plain)]
pub struct Node<S> {
    pub p: Plain<S>,
}

fn main() {}
