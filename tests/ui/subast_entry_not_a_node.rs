//! A `#[visiting(..)]` entry has to be an AST node itself. Without the check the omission surfaces
//! only once a `define_visit!` tries to expand the entry's metadata macro, as `cannot find `Plain` in
//! `$crate`` — a macro the user never wrote, reported far from the attribute at fault.

use visiting::Visiting;

pub struct Plain<S>(S);

#[derive(Visiting)]
#[visiting(crate::Plain)]
pub struct Node<S> {
    pub p: crate::Plain<S>,
}

fn main() {}
