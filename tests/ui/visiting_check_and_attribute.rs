//! The follow-list has one home. Written as both an attribute and a check, the type would be marked
//! twice, and the duplicate `impl Visiting` is reported against generated code rather than against
//! either spelling.
//!
//! `macro_name = ..` is here only to keep the two trampolines apart: the derive and `#[announce]`
//! both publish a macro under the type's own name, and that collision is a separate complaint.

#[announce::announce(visiting::visiting())]
pub struct Leaf {
    pub n: u32,
}

#[derive(visiting::Visiting)]
#[visiting(crate::Leaf)]
#[announce::announce(macro_name = DocAnnounced, visiting::visiting(crate::Leaf))]
pub struct Doc {
    pub leaf: crate::Leaf,
}

fn main() {}
