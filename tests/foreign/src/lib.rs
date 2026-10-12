//! Types for another crate to walk. Nothing here derives `Visiting`, implements it, or builds a
//! visitor: the only mention of this crate's walker is the `visiting(..)` check handed to
//! `#[announce]`, which marks each type where the orphan rule allows it — beside the definition.
//!
//! Field types are `crate::`-rooted and bounds absolute because announce re-uses the payload in the
//! consuming crate.

/// A leaf, with no follow-list.
#[derive(Debug)]
#[announce::announce(visiting::visiting())]
pub struct Leaf {
    pub n: u32,
}

/// A node reaching `Leaf`. The follow-list is written `crate::Leaf` here and must arrive in the
/// consuming crate still naming *this* crate.
#[derive(Debug)]
#[announce::announce(visiting::visiting(crate::Leaf))]
pub struct Tree {
    pub head: crate::Leaf,
    pub rest: ::std::vec::Vec<crate::Leaf>,
}

/// A node in a submodule, so the path the consumer fetches by is more than two segments.
pub mod inner {
    #[announce::announce(visiting::visiting(crate::Leaf))]
    pub struct Branch {
        pub leaf: crate::Leaf,
    }
}

/// An alias, so the node below can spell its field under a different name than the type's own.
pub mod alias {
    pub type Twin = crate::Leaf;
}

/// The aliased form of a follow entry, `path as Alias`: the entry names the type, the field spells
/// the alias, and the walk matches a field head by its last segment.
#[announce::announce(visiting::visiting(crate::Leaf as Twin))]
pub struct Aliased {
    pub twin: crate::alias::Twin,
}
