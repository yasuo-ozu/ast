//! The `visiting(..)` check marks a node, and only a struct or an enum can be one. `#[announce]`
//! also takes a trait, a type alias and an inline `mod`, so the rule has to be stated at the check
//! rather than left to surface at the visitor as a missing definition.

#[announce::announce(visiting::visiting())]
pub trait Greet {
    fn hi(&self) -> u32;
}

fn main() {}
