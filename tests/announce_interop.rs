//! A type carrying only `#[announce]` — no derive of ours — walked by a generated visitor.
//!
//! `announce` and this crate solve the same problem the same way: a `#[macro_export]`ed
//! `macro_rules!` beside the item, carrying the item's tokens to a macro in another crate. They
//! spelled the handshake differently, so this crate's trampoline now answers announce's calling
//! convention too and every fetch uses it. The payoff is here: `Leaf` knows nothing about
//! `visiting`, and the walk reaches it anyway.
#![allow(dead_code)]

use visiting::Visiting;

// Not `#[derive(Visiting)]`. The only thing on it is announce.
#[announce::announce]
pub struct Leaf {
    pub n: u32,
}

// The one piece still required by hand: `#[visiting(..)]` entries must carry the marker trait, and
// `#[announce]` does not know to implement it. Fine for a type in this crate; for one from a crate
// that only announces, the orphan rule would forbid it, so the bound would have to be relaxed for
// announce-sourced entries. That is the next thing to decide.
impl Visiting for Leaf {}

#[derive(Visiting)]
#[visiting(crate::Leaf)]
pub struct Doc {
    pub leaf: Leaf,
}

pub mod visit {
    visiting::define_visit!(crate::Doc, crate::Leaf);
}

#[test]
fn an_announced_type_is_walked() {
    let d = Doc {
        leaf: Leaf { n: 7 },
    };
    let mut seen = Vec::new();
    d.visit(|l: &Leaf| seen.push(l.n));
    assert_eq!(seen, vec![7], "the announced leaf was reached");
}
