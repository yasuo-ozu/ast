//! A type announced with the check must not also derive `Visiting`. The check owns `#[visiting]`,
//! `#[seq]`, `#[opt]` and `#[skip]`, so announce strips them from the item it emits — before the
//! derive runs, which would then silently lose its own list. Two marker impls would collide too.
//!
//! `macro_name = ..` is here only to keep the two trampolines apart: the derive and `#[announce]`
//! both publish a macro under the type's own name, and that collision is a separate complaint.

#[announce::announce(visiting::visiting)]
pub struct Leaf {
    pub n: u32,
}

#[derive(visiting::Visiting)]
#[announce::announce(macro_name = DocAnnounced, visiting::visiting(crate::Leaf))]
pub struct Doc {
    pub leaf: crate::Leaf,
}

fn main() {}
