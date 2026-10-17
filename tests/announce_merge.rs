//! `#[derive(Visiting)]` announces the node itself, so a type that is also announced for another
//! reader — `#[announce]` or `#[derive(Announce)]` — has two announcements. The derive stands aside
//! for the other one, which then carries the follow-list and the field markers in its payload; one
//! trampoline serves the visitor and every other announce consumer.
//!
//! The other announcement checks the item's paths as announce always does, so these definitions
//! write them absolutely. A node that only derives keeps writing them however it likes.
#![allow(dead_code)]

use visiting::Visiting;

/// `#[announce]` above the derive: it moves below the derive before it announces, so the derive
/// sees it and leaves the trampoline to it.
#[announce::announce]
#[derive(Visiting)]
#[visiting(crate::Leaf)]
pub struct Doc {
    #[seq]
    pub leaves: ::std::vec::Vec<crate::Leaf>,
    #[skip]
    pub note: crate::Leaf,
}

/// `#[announce]` below the derive.
#[derive(Visiting)]
#[announce::announce]
pub struct Leaf {
    pub n: u32,
}

/// A `#[derive(Announce)]` of its own below the derive. Not next to it: rustfmt merges adjacent
/// derives into one list, and the derive cannot see one in its own list.
#[derive(Visiting)]
#[visiting(crate::Leaf)]
#[derive(announce::Announce)]
pub enum Wrap {
    One(crate::Leaf),
    Doc(::std::boxed::Box<crate::Doc>),
}

pub mod v {
    visiting::define_visit!(crate::Wrap, crate::Doc, crate::Leaf);
}

#[test]
fn one_announcement_serves_the_visitor() {
    let d = Doc {
        leaves: vec![Leaf { n: 1 }, Leaf { n: 2 }],
        note: Leaf { n: 9 },
    };
    let w = Wrap::Doc(Box::new(d));
    let mut seen = Vec::new();
    w.visit(|l: &Leaf| seen.push(l.n));
    assert_eq!(
        seen,
        vec![1, 2],
        "the follow-list and `#[skip]` came through the payload"
    );
}

#[test]
fn each_is_announced_once() {
    fn announced<T: announce::Announce>() {}
    announced::<Doc>();
    announced::<Leaf>();
    announced::<Wrap>();
}

/// Paths that announce would reject, taken through the derive's own announcement and back: `super::`,
/// a module-relative path, a const array length, a qualified path and a bounded parameter. The walk below only
/// works if the visitor sees each one as written. The bounds are resolvable paths, because the
/// visitor's own items repeat them in `mod v` — the rule for every node, announced or not.
/// A leaf of its own: a type belongs to one `define_visit!` only.
#[derive(Visiting)]
pub struct Twig {
    pub n: u32,
}

pub mod paths {
    use visiting::Visiting;

    pub const N: usize = 2;
    pub trait Has {
        type Out;
    }
    impl Has for () {
        type Out = u8;
    }

    #[derive(Visiting)]
    #[visiting(crate::Twig)]
    pub struct List<T>
    where
        T: crate::paths::Has + Clone + ::core::default::Default,
    {
        pub next: Option<Box<List<T>>>,
        pub up: super::Twig,
        pub pair: [super::Twig; N],
        pub out: <T as Has>::Out,
    }

    pub mod v {
        visiting::define_visit!(crate::paths::List, crate::Twig);
    }
}

#[test]
fn relative_paths_survive_the_announcement() {
    use paths::List;
    let leaf = |n| Twig { n };
    let l: List<()> = List {
        next: Some(Box::new(List {
            next: None,
            up: leaf(3),
            pair: [leaf(4), leaf(5)],
            out: 0,
        })),
        up: leaf(0),
        pair: [leaf(1), leaf(2)],
        out: 0,
    };
    // A bounded parameter makes the visitor heterogeneous, which takes a struct, not a closure.
    struct Seen(Vec<u32>);
    impl paths::v::Visit for Seen {
        fn visit_twig(&mut self, i: &Twig) {
            self.0.push(i.n);
        }
    }
    impl paths::v::VisitMut for Seen {}
    let mut seen = Seen(Vec::new());
    l.visit(&mut seen);
    assert_eq!(seen.0, vec![3, 4, 5, 0, 1, 2]);
}
