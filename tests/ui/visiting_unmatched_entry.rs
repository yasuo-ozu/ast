//! A follow-list entry that matches no field does nothing: the walk descends by matching a field's
//! head against the entry's last segment, so an entry naming a type no field spells is dead
//! configuration. `node_marker` skips such an entry rather than asserting it is a node, so without
//! this diagnostic nothing at all would be said about it.
//!
//! It is a warning, not an error — the entry is inert, not wrong — and a proc macro has no stable
//! way to raise one, so it is emitted as a deprecated item used once. `deny` is what lets a `trybuild`
//! fixture pin a warning.
#![deny(deprecated)]

#[derive(visiting::Visiting)]
pub struct Leaf {
    pub n: u32,
}

// `Twig` is the dead entry; `crate::Leaf` beside it is matched, and must stay silent.
#[derive(visiting::Visiting)]
#[visiting(crate::Leaf, crate::Twig)]
pub struct Node {
    pub leaf: crate::Leaf,
}

#[derive(visiting::Visiting)]
pub struct Twig {
    pub n: u32,
}

fn main() {}
