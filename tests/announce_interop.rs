//! Nodes declared through `#[announce]`, walked by a generated visitor.
//!
//! announce puts a `macro_rules!` trampoline beside the item, carrying its tokens to a macro in any
//! crate, and every fetch a visitor makes goes through it. The attributes a node is written with,
//! which rustc does not know, come from an announce *check*, invoked beside the definition. The
//! check's arguments carry the follow-list here; `tests/announce_foreign.rs` writes it as
//! `#[visiting(..)]` inside the definition instead, and uses the field markers.
//!
//! Field types are written `crate::Leaf` rather than `Leaf`, and bounds absolutely, because
//! announce's payload is re-used in another crate: that is its own rule, and it applies to any
//! announced item.
#![allow(dead_code)]

/// A leaf: no follow-list, so the check takes no arguments. The bare path form,
/// `#[announce(visiting::visiting)]`, means the same.
#[visiting::visiting]
#[derive(Debug)]
pub struct Leaf {
    pub n: u32,
}

/// A node that reaches `Leaf`. The follow-list is passed to the check instead of `#[visiting(..)]`,
/// and the attributes the announced item does carry (a derive, a doc comment) ride along without
/// disturbing the walk.
#[visiting::visiting(crate::Leaf)]
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
fn every_node_is_announced() {
    fn announced<T: announce::Announce>() {}
    announced::<Leaf>();
    announced::<Doc>();
}

/// A generic node, to pin that the code the check writes carries the definition's generics and
/// where-clause rather than only applying to a concrete type.
///
/// `crate = ..` names the crate that code reaches us through — the facade form, spelled here as this
/// crate itself, which is what it would have said anyway.
#[visiting::visiting(crate = ::visiting, crate::Leaf)]
// One bound inline and one in the where-clause: the follow-list assertion the check writes repeats
// both, so it holds under exactly the conditions the node exists under.
pub struct Wrapper<T: ::core::clone::Clone, U>
where
    U: ::core::default::Default,
{
    pub inner: crate::Leaf,
    pub extra: T,
    pub more: U,
}

#[test]
fn a_generic_node_is_announced_too() {
    fn announced<T: announce::Announce>() {}
    announced::<Wrapper<u8, u16>>();
}

/// A second leaf, for the case below. Its own type, because each `define_visit!`
/// gives every type it visits an inherent `visit`, so two visitors cannot share one.
///
/// Written in the bare-path form, with no argument list at all — a check announce invokes as
/// `visiting! { (), ITEM }`.
#[visiting::visiting]
pub struct Twig {
    pub n: u32,
}

/// A node with a follow-list in `#[visiting(..)]`, reaching one with none.
#[visiting::visiting]
#[visiting(crate::Twig)]
pub struct Branch {
    pub twig: crate::Twig,
}

pub mod visit_mixed {
    visiting::define_visit!(crate::Branch, crate::Twig);
}

#[test]
fn a_follow_list_in_the_attribute_reaches_a_leaf() {
    let d = Branch {
        twig: Twig { n: 3 },
    };
    let mut seen = Vec::new();
    d.visit(|t: &Twig| seen.push(t.n));
    assert_eq!(seen, vec![3]);
}
