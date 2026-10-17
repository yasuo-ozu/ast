//! The descent goes through [`Walk`](visiting::walk::Walk): the container impls in `ast` peel the
//! wrapper layers and each visited node's generated impl bottoms out in its `visit_*` method.
//!
//! Each call carries an indicator computed from the field's shape, so the container impls stay generic
//! in what they hold: `Vec<(Length, Line)>` is walked at `Thru<(Skip, Here)>`. A leaf sharing a tuple
//! with a node is just `Skip` and needs no impl of its own, and two tuple shapes of one arity are the
//! same impl at different indicators — which the `collides` module at the bottom pins.
#![allow(dead_code)]

mod ast {

    /// A consumer leaf that shares a tuple with a followed node — the `Vec<(Length, Line)>` shape.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Length(pub i64);

    #[announce::announce(visiting::visiting)]
    #[derive(Debug)]
    pub enum Line {
        Text(i64),
        Nested(::std::boxed::Box<crate::ast::Line>),
    }

    #[announce::announce(visiting::visiting)]
    #[derive(Debug)]
    #[visiting(crate::ast::Line)]
    pub struct Page {
        pub placed: ::std::vec::Vec<(crate::ast::Length, crate::ast::Line)>,
        pub header: ::core::option::Option<::std::boxed::Box<crate::ast::Line>>,
        pub by_name: ::std::collections::BTreeMap<::std::string::String, crate::ast::Line>,
        pub width: crate::ast::Length,
    }

    pub mod v {
        visiting::define_visit!(crate::ast::Line, crate::ast::Page);
    }
}

use ast::{Length, Line, Page};

fn sample() -> Page {
    Page {
        placed: vec![
            (Length(1), Line::Text(10)),
            (Length(2), Line::Nested(Box::new(Line::Text(20)))),
        ],
        header: Some(Box::new(Line::Text(30))),
        by_name: [("a".to_owned(), Line::Text(40))].into_iter().collect(),
        width: Length(99),
    }
}

fn texts(p: &Page) -> Vec<i64> {
    let mut v = Vec::new();
    p.visit(|l: &Line| {
        if let Line::Text(x) = l {
            v.push(*x)
        }
    });
    v.sort_unstable();
    v
}

#[test]
fn every_container_shape_descends() {
    // 20 sits under a Box inside a tuple inside a Vec — four layers.
    assert_eq!(texts(&sample()), vec![10, 20, 30, 40]);
}

#[test]
fn the_leaf_beside_a_node_is_untouched() {
    let mut p = sample();
    p.visit_mut(|l: &mut Line| {
        if let Line::Text(x) = l {
            *x += 1
        }
    });
    assert_eq!(texts(&p), vec![11, 21, 31, 41]);
    assert_eq!(p.placed[0].0, Length(1), "the tuple's leaf is not visited");
    assert_eq!(p.width, Length(99));
}

#[test]
fn a_node_counts_itself_and_its_children() {
    let mut n = 0usize;
    sample().visit(|_: &Line| n += 1);
    assert_eq!(n, 5, "4 texts + the Nested wrapper");
}

// ── when two tuple shapes of one arity overlap ───────────────────────────────────────────────────
mod collides {

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Mark(pub i64);

    #[announce::announce(visiting::visiting)]
    #[derive(Debug)]
    pub enum Node {
        N(i64),
    }

    /// Two shapes of one arity, which an earlier design could not give separate impls: they are the
    /// same tuple impl at `(Here, Here)` and `(Skip, Here)`.
    #[announce::announce(visiting::visiting)]
    #[derive(Debug)]
    #[visiting(crate::collides::Node)]
    pub struct Holder {
        pub both: ::std::vec::Vec<(crate::collides::Node, crate::collides::Node)>,
        pub tagged: ::std::vec::Vec<(crate::collides::Mark, crate::collides::Node)>,
    }

    pub mod v {
        visiting::define_visit!(crate::collides::Node, crate::collides::Holder);
    }

    #[test]
    fn the_fallback_walks_both_shapes_correctly() {
        let h = Holder {
            both: vec![(Node::N(1), Node::N(2))],
            tagged: vec![(Mark(99), Node::N(3))],
        };
        let mut seen = Vec::new();
        h.visit(|n: &Node| {
            let Node::N(x) = n;
            seen.push(*x)
        });
        seen.sort_unstable();
        assert_eq!(
            seen,
            vec![1, 2, 3],
            "both slots of the pinned shape, one of the tagged"
        );
    }

    #[test]
    fn the_leaf_is_still_not_mutated() {
        let mut h = Holder {
            both: vec![],
            tagged: vec![(Mark(99), Node::N(3))],
        };
        h.visit_mut(|n: &mut Node| {
            let Node::N(x) = n;
            *x = 0
        });
        assert_eq!(h.tagged[0].0, Mark(99));
    }
}
