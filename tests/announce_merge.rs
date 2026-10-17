//! A node that another reader announces too — a `#[coinduction]` module exports a type by announcing
//! it — has two `#[announce]` attributes. They merge into one trampoline, which carries the check's
//! follow-list and the field markers to the visitor and serves every other announce consumer.
#![allow(dead_code)]

/// The check's announcement first.
#[announce::announce(visiting::visiting)]
#[announce::announce]
#[visiting(crate::Leaf)]
pub struct Doc {
    #[seq]
    pub leaves: ::std::vec::Vec<crate::Leaf>,
    #[skip]
    pub note: crate::Leaf,
}

/// The other reader's announcement first.
#[announce::announce]
#[announce::announce(visiting::visiting)]
pub struct Leaf {
    pub n: u32,
}

pub mod v {
    visiting::define_visit!(crate::Doc, crate::Leaf);
}

#[test]
fn one_announcement_serves_the_visitor() {
    let d = Doc {
        leaves: vec![Leaf { n: 1 }, Leaf { n: 2 }],
        note: Leaf { n: 9 },
    };
    let mut seen = Vec::new();
    d.visit(|l: &Leaf| seen.push(l.n));
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
}

/// A leaf of its own: a type belongs to one `define_visit!` only.
#[announce::announce(visiting::visiting)]
pub struct Twig {
    pub n: u32,
}

/// Absolute paths of every kind, taken through the announcement: a qualified path, a const array
/// length and bounded parameters. The walk below only works if the visitor sees each one as
/// written.
pub mod paths {

    pub const N: usize = 2;
    pub trait Has {
        type Out;
    }
    impl Has for () {
        type Out = u8;
    }

    #[announce::announce(visiting::visiting)]
    #[visiting(crate::Twig)]
    pub struct List<T>
    where
        T: crate::paths::Has + ::core::clone::Clone + ::core::default::Default,
    {
        pub next: ::core::option::Option<::std::boxed::Box<crate::paths::List<T>>>,
        pub up: crate::Twig,
        pub pair: [crate::Twig; crate::paths::N],
        pub out: <T as crate::paths::Has>::Out,
    }

    pub mod v {
        visiting::define_visit!(crate::paths::List, crate::Twig);
    }
}

#[test]
fn absolute_paths_reach_the_visitor() {
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
