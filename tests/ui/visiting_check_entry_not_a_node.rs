//! A follow-list entry has to be a node itself, whichever spelling declared the list. Without the
//! assertion the omission surfaces only when a `define_visit!` tries to fetch the entry, as a
//! missing macro in a module far from the list at fault.

pub struct Plain {
    pub n: u32,
}

#[announce::announce(visiting::visiting(crate::Plain))]
pub struct Doc {
    pub plain: crate::Plain,
}

fn main() {}
