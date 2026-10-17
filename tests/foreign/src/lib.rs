//! Types for another crate to walk. Nothing here builds a visitor: the only mention of the walker
//! is `#[visiting]`, which announces each type, and this crate does not depend on announce itself.
//!
//! Field types are `crate::`-rooted and bounds absolute because announce re-uses the payload in the
//! consuming crate.

/// A leaf, with no follow-list.
#[derive(Debug)]
#[visiting::visiting]
pub struct Leaf {
    pub n: u32,
}

/// A node reaching `Leaf`. The follow-list is written `crate::Leaf` here and must arrive in the
/// consuming crate still naming *this* crate.
#[derive(Debug)]
#[visiting::visiting(crate::Leaf)]
pub struct Tree {
    pub head: crate::Leaf,
    pub rest: ::std::vec::Vec<crate::Leaf>,
}

/// A node in a submodule, so the path the consumer fetches by is more than two segments.
pub mod inner {
    #[visiting::visiting(crate::Leaf)]
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
#[visiting::visiting(crate::Leaf as Twin)]
pub struct Aliased {
    pub twin: crate::alias::Twin,
}

/// The follow-list written in a second `#[visiting(..)]` on the definition rather than in the
/// attribute's arguments, together with the field markers `#[seq]`, `#[opt]` and `#[skip]`.
///
/// rustc knows none of the markers. The check `#[visiting]` hands to `#[announce]` owns them:
/// announce strips them from the item it emits and keeps them in the payload a visitor reads.
#[visiting::visiting]
#[visiting(crate::Leaf)]
pub struct Marked {
    #[seq]
    pub many: ::std::vec::Vec<crate::Leaf>,
    #[opt]
    pub maybe: ::core::option::Option<crate::Leaf>,
    #[skip]
    pub hidden: crate::Leaf,
}
