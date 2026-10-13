//! Types carrying only `#[announce]` — no derive of ours — walked by a generated visitor.
//!
//! `announce` and this crate solve the same problem the same way: a `#[macro_export]`ed
//! `macro_rules!` beside the item, carrying the item's tokens to a macro in another crate. They
//! spelled the handshake differently, so this crate's trampoline now answers announce's calling
//! convention and every fetch uses it.
//!
//! The rest is the declaration: the `Visiting` marker, which only the defining crate may write, and
//! the attributes a node is written with, which rustc rejects where no derive declares them. Both
//! come from an announce *check*, invoked beside the definition. Nothing below derives anything of
//! ours. The check's arguments carry the follow-list here; `tests/announce_foreign.rs` writes it as
//! `#[visiting(..)]` inside the definition instead, and uses the field markers.
//!
//! Field types are written `crate::Leaf` rather than `Leaf`, and bounds absolutely, because
//! announce's payload is re-used in another crate: that is its own rule, and it applies to any
//! announced item.
#![allow(dead_code)]

/// A leaf: no follow-list, so the check takes no arguments. The bare path form,
/// `#[announce(visiting::visiting)]`, means the same.
#[announce::announce(visiting::visiting())]
#[derive(Debug)]
pub struct Leaf {
    pub n: u32,
}

/// A node that reaches `Leaf`. The list `#[derive(Visiting)]` would take in `#[visiting(..)]` is
/// passed to `#[announce]` instead, and the attributes the announced item does carry (a derive, a
/// doc comment) ride along without disturbing the walk.
#[announce::announce(visiting::visiting(crate::Leaf))]
#[derive(Debug)]
pub struct Doc {
    /// The leaf.
    pub leaf: crate::Leaf,
    pub tail: ::std::vec::Vec<crate::Leaf>,
}

pub mod visit {
    visiting::define_visit!(crate::Doc, crate::Leaf);
}

#[test]
fn a_visitor_is_built_from_announce_declarations_alone() {
    let d = Doc {
        leaf: Leaf { n: 7 },
        tail: vec![Leaf { n: 8 }, Leaf { n: 9 }],
    };
    let mut seen = Vec::new();
    d.visit(|l: &Leaf| seen.push(l.n));
    assert_eq!(seen, vec![7, 8, 9], "every announced leaf was reached");
}

#[test]
fn the_check_supplies_the_marker_trait() {
    // Not written anywhere in this file: the check emitted it, beside each definition.
    fn is_node<T: visiting::visit::Visiting>() {}
    is_node::<Leaf>();
    is_node::<Doc>();
}

/// A generic node, to pin that the marker impl the check writes carries the definition's generics
/// and where-clause rather than only applying to a concrete type.
///
/// `crate = ..` names the crate the marker impl reaches us through — the facade form, spelled here
/// as this crate itself, which is what the impl would have said anyway.
#[announce::announce(visiting::visiting(crate = ::visiting, crate::Leaf))]
// One bound inline and one in the where-clause: the marker impl has to reproduce both, so the node
// is marked under exactly the conditions it exists under.
pub struct Wrapper<T: ::core::clone::Clone, U>
where
    U: ::core::default::Default,
{
    pub inner: crate::Leaf,
    pub extra: T,
    pub more: U,
}

#[test]
fn a_generic_node_is_marked_too() {
    fn is_node<T: visiting::visit::Visiting>() {}
    is_node::<Wrapper<u8, u16>>();
}

/// A second announced leaf, for the mixed case below. Its own type, because each `define_visit!`
/// gives every type it visits an inherent `visit`, so two visitors cannot share one.
///
/// Written in the bare-path form, with no argument list at all — a check announce invokes as
/// `visiting! { (), ITEM }`.
#[announce::announce(visiting::visiting)]
pub struct Twig {
    pub n: u32,
}

/// The mixed case: our derive on a node whose child is announced by someone else. This is what used
/// to need `impl Visiting for Twig {}` by hand — and what the orphan rule makes impossible once the
/// announced type lives in another crate.
#[derive(visiting::Visiting)]
#[visiting(crate::Twig)]
pub struct Derived {
    pub twig: Twig,
}

pub mod visit_mixed {
    visiting::define_visit!(crate::Derived, crate::Twig);
}

#[test]
fn a_derived_node_reaches_an_announced_one() {
    let d = Derived {
        twig: Twig { n: 3 },
    };
    let mut seen = Vec::new();
    d.visit(|t: &Twig| seen.push(t.n));
    assert_eq!(seen, vec![3]);
}
