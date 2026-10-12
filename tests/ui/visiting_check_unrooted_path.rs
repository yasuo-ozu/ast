//! A follow-list entry must be fully qualified wherever the list is written. The check's list ends
//! up in the trampoline's metadata and is expanded in the *consumer's* module, so a bare ident
//! means something else there — the same rule, and the same error, as the attribute's.

#[announce::announce(visiting::visiting())]
pub struct Leaf {
    pub n: u32,
}

#[announce::announce(visiting::visiting(Leaf))]
pub struct Doc {
    pub leaf: crate::Leaf,
}

fn main() {}
