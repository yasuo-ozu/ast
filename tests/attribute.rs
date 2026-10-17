//! `#[visiting]` imported by name. A further `#[visiting(..)]` on the item then names the attribute
//! macro too, so the first one folds them into its own arguments before rustc would expand them.
#![allow(dead_code)]

use visiting::visiting;

#[visiting]
#[derive(Debug)]
pub struct Leaf {
    pub n: u32,
}

/// Not listed in the visitor: the walk passes through it only because `Outer` follows it.
#[visiting]
pub struct Mid {
    pub leaf: crate::Leaf,
}

/// The follow-list split between the attribute's arguments and a second attribute below it.
#[visiting(crate::Leaf)]
#[visiting(crate::Mid)]
pub struct Outer {
    pub direct: crate::Leaf,
    pub via: crate::Mid,
}

pub mod v {
    visiting::define_visit!(crate::Outer, crate::Leaf);
}

#[test]
fn both_follow_lists_are_kept() {
    let o = Outer {
        direct: Leaf { n: 1 },
        via: Mid {
            leaf: Leaf { n: 2 },
        },
    };
    let mut seen = Vec::new();
    o.visit(|l: &Leaf| seen.push(l.n));
    assert_eq!(
        seen,
        vec![1, 2],
        "`Mid` was followed through the second attribute"
    );
}

#[test]
fn a_derive_below_still_runs() {
    assert_eq!(format!("{:?}", Leaf { n: 3 }), "Leaf { n: 3 }");
}
