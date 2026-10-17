//! A `#[visiting(..)]` entry has to be announced itself. Without the check the omission surfaces
//! only once a `define_visit!` tries to expand the entry's trampoline, as `cannot find `Plain` in
//! `$crate`` — a macro the user never wrote, reported far from the attribute at fault.

pub struct Plain<S>(S);

#[visiting::visiting]
#[visiting(crate::Plain)]
pub struct Node<S> {
    pub p: crate::Plain<S>,
}

fn main() {}
