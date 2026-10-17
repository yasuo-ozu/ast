//! A visitor reached through a wrapper, and several closures in one traversal.
//!
//! A `Box` around a visitor is a visitor, so `node.visit(Box::new(pass))` works without unwrapping;
//! a consumer's own wrapper writes the same one-line forwarding impl. A tuple is a visitor only
//! when every element is a closure — those fold into one hook, and so into one walk. Two struct
//! passes are two walks, written as two calls.
#![allow(dead_code)]

mod ast {

    #[visiting::visiting]
    #[derive(Debug)]
    pub enum Node {
        Leaf(i64),
    }

    #[visiting::visiting]
    #[derive(Debug)]
    #[visiting(crate::ast::Node)]
    pub struct Tree {
        pub nodes: ::std::vec::Vec<crate::ast::Node>,
    }

    pub mod v {
        visiting::define_visit!(crate::ast::Node, crate::ast::Tree);
    }
}

use ast::{Node, Tree};

fn sample() -> Tree {
    Tree {
        nodes: vec![Node::Leaf(1), Node::Leaf(2), Node::Leaf(3)],
    }
}

#[derive(Default)]
struct Count(usize);
impl ast::v::Visit for Count {
    fn visit_node(&mut self, _: &Node) {
        self.0 += 1;
    }
}

#[derive(Default)]
struct Sum(i64);
impl ast::v::Visit for Sum {
    fn visit_node(&mut self, n: &Node) {
        let Node::Leaf(x) = n;
        self.0 += x;
    }
}

#[test]
fn two_visitors_are_two_walks() {
    let (mut count, mut sum) = (Count::default(), Sum::default());
    let t = sample();
    t.visit(&mut count);
    t.visit(&mut sum);
    assert_eq!((count.0, sum.0), (3, 6));
}

/// `visit` takes the visitor by value, so a wrapper is moved in — the tally comes back through a
/// borrow the visitor holds.
struct Tally<'a>(&'a mut usize);
impl ast::v::Visit for Tally<'_> {
    fn visit_node(&mut self, _: &Node) {
        *self.0 += 1;
    }
}

#[test]
fn a_boxed_visitor_is_a_visitor() {
    let mut n = 0usize;
    sample().visit(Box::new(Tally(&mut n)));
    assert_eq!(n, 3);
}

#[test]
fn a_consumer_wrapper_forwarding_to_a_visitor_works() {
    struct Holder<V>(V);
    impl<V: ast::v::Visit> ast::v::Visit for Holder<V> {
        fn visit_node(&mut self, i: &Node) {
            self.0.visit_node(i);
        }
    }

    let mut n = 0usize;
    sample().visit(Holder(Tally(&mut n)));
    assert_eq!(n, 3);
}

// The point of folding a tuple of closures into one hook: both run at each node, in tuple order,
// before the walk moves on. Counting callbacks cannot tell that apart from two separate walks —
// only the ORDER can, so this records it. `ababab` is one walk; `aaabbb` would be two.
#[test]
fn a_tuple_of_closures_interleaves_at_each_node() {
    use core::cell::RefCell;
    let order = RefCell::new(String::new());
    sample().visit((
        |_: &Node| order.borrow_mut().push('a'),
        |_: &Node| order.borrow_mut().push('b'),
    ));
    assert_eq!(
        order.into_inner(),
        "ababab",
        "both closures at each of the three nodes, not one closure's whole traversal and then the other's"
    );
}

#[test]
fn closures_and_tuples_of_closures_still_work() {
    let (mut n, mut total) = (0usize, 0i64);
    sample().visit((
        |_: &Node| n += 1,
        |x: &Node| {
            let Node::Leaf(v) = x;
            total += v;
        },
    ));
    assert_eq!((n, total), (3, 6));
}

#[test]
fn two_edit_passes_run_in_order() {
    struct Double;
    impl ast::v::VisitMut for Double {
        fn visit_node_mut(&mut self, n: &mut Node) {
            let Node::Leaf(x) = n;
            *x *= 2;
        }
    }
    struct Bump;
    impl ast::v::VisitMut for Bump {
        fn visit_node_mut(&mut self, n: &mut Node) {
            let Node::Leaf(x) = n;
            *x += 1;
        }
    }

    let mut t = sample();
    t.visit_mut(&mut Double);
    t.visit_mut(&mut Bump);
    let mut got = Vec::new();
    t.visit(|n: &Node| {
        let Node::Leaf(x) = n;
        got.push(*x);
    });
    assert_eq!(got, vec![3, 5, 7], "Double over the whole tree, then Bump");
}

// A `use path::to::visit::*;` must bring in the public API and nothing else. The closure adapters,
// the driver and the module's walk tag are private to the generated module, so a glob cannot reach
// them and cannot shadow a name of the caller's own. (`#[doc(hidden)]` alone would not do this — it
// hides an item from rustdoc while leaving it importable.)
mod glob_import {

    #[visiting::visiting]
    pub enum Node {
        Leaf(i64),
    }

    pub mod v {
        visiting::define_visit!(super::Node);
    }

    use v::*;

    // Same names as the generated machinery. These compile only because the glob does not import it.
    pub struct Driver;
    pub struct DriverMut;
    pub struct NodeHook;
    pub struct NodeHookMut;
    pub struct __VisitingWalkTag;
    pub trait Hook {}
    pub trait HookMut {}
    pub trait IntoHook {}
    pub trait IntoHookMut {}

    #[test]
    fn a_glob_import_brings_in_the_public_api_only() {
        // `Visit` and the free fns did come through the glob, and still work.
        struct Count(usize);
        impl Visit for Count {
            fn visit_node(&mut self, i: &Node) {
                self.0 += 1;
                visit_node(self, i);
            }
        }
        let mut c = Count(0);
        Node::Leaf(1).visit(&mut c);
        assert_eq!(c.0, 1);
    }
}
