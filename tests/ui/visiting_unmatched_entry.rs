//! A follow-list entry that matches no field does nothing: the walk descends by matching a field's
//! head against the entry's last segment, so an entry naming a type no field spells is dead
//! configuration. The check skips such an entry rather than asserting it is announced, so without
//! this diagnostic nothing at all would be said about it.
//!
//! It is a warning, not an error — the entry is inert, not wrong — and a proc macro has no stable
//! way to raise one, so it is emitted as a deprecated item used once. `deny` is what lets a `trybuild`
//! fixture pin a warning.
#![deny(deprecated)]

#[announce::announce(visiting::visiting)]
pub struct Leaf {
    pub n: u32,
}

// `Twig` is the dead entry; `crate::Leaf` beside it is matched, and must stay silent.
#[announce::announce(visiting::visiting)]
#[visiting(crate::Leaf, crate::Twig)]
pub struct Node {
    pub leaf: crate::Leaf,
}

#[announce::announce(visiting::visiting)]
pub struct Twig {
    pub n: u32,
}

fn main() {}
