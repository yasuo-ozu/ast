//! Every shape `visit()` accepts, against every level of an inheritance chain.
//!
//! A closure or a tuple is generated per visitor, so the question each test answers is whether the
//! shape still reaches a node type the visitor *inherits* rather than lists — including one several
//! levels up, and from an entry point partway down the chain.
#![allow(dead_code)]

use ast::Ast;
use core::marker::PhantomData;

#[derive(Ast)]
pub enum Type<S> {
    U(PhantomData<S>),
}
#[derive(Ast)]
#[subast(crate::Type)]
pub struct Expr<S> {
    pub t: Type<S>,
}
#[derive(Ast)]
#[subast(crate::Expr)]
pub struct Stmt<S> {
    pub e: Expr<S>,
}
#[derive(Ast)]
#[subast(crate::Stmt)]
pub struct Item<S> {
    pub s: Stmt<S>,
}

pub mod base {
    ast::visitor!(crate::Type, crate::Expr);
}
pub mod mid {
    ast::visitor!(crate::base => crate::Stmt);
}
pub mod top {
    ast::visitor!(crate::mid => crate::Item);
}

fn sample() -> Item<()> {
    Item {
        s: Stmt {
            e: Expr {
                t: Type::U(PhantomData),
            },
        },
    }
}

// A closure reaches its own level and every level above it.

#[test]
fn closure_over_each_level() {
    let mut n = 0;
    sample().visit(|_: &Item<()>| n += 1);
    sample().visit(|_: &Stmt<()>| n += 1);
    sample().visit(|_: &Expr<()>| n += 1);
    sample().visit(|_: &Type<()>| n += 1);
    assert_eq!(n, 4);
}

#[test]
fn mut_closure_over_an_inherited_type() {
    let mut n = 0;
    sample().visit_mut(|_: &mut Type<()>| n += 1);
    assert_eq!(n, 1);
}

// A tuple of closures may mix levels freely, up to the arity limit of eight.

#[test]
fn tuple_mixes_levels() {
    let (mut a, mut b, mut c, mut d) = (0, 0, 0, 0);
    sample().visit((
        |_: &Item<()>| a += 1,
        |_: &Stmt<()>| b += 1,
        |_: &Expr<()>| c += 1,
        |_: &Type<()>| d += 1,
    ));
    assert_eq!((a, b, c, d), (1, 1, 1, 1));
}

#[test]
fn tuple_of_two_inherited_types_only() {
    let (mut a, mut b) = (0, 0);
    sample().visit((|_: &Stmt<()>| a += 1, |_: &Type<()>| b += 1));
    assert_eq!((a, b), (1, 1));
}

#[test]
fn tuple_at_full_arity() {
    let mut c = [0u32; 8];
    {
        let [a, b, d, e, f, g, h, i] = &mut c;
        sample().visit((
            |_: &Item<()>| *a += 1,
            |_: &Stmt<()>| *b += 1,
            |_: &Expr<()>| *d += 1,
            |_: &Type<()>| *e += 1,
            |_: &Item<()>| *f += 1,
            |_: &Stmt<()>| *g += 1,
            |_: &Expr<()>| *h += 1,
            |_: &Type<()>| *i += 1,
        ));
    }
    assert_eq!(c, [1; 8]);
}

#[test]
fn mut_tuple_mixes_levels() {
    let (mut a, mut b) = (0, 0);
    sample().visit_mut((|_: &mut Item<()>| a += 1, |_: &mut Type<()>| b += 1));
    assert_eq!((a, b), (1, 1));
}

// Entering partway down the chain uses that level's visitor, which still reaches above itself.

#[test]
fn an_ancestors_own_entry_point() {
    let s: Stmt<()> = Stmt {
        e: Expr {
            t: Type::U(PhantomData),
        },
    };
    let (mut a, mut b, mut c) = (0, 0, 0);
    s.visit((
        |_: &Stmt<()>| a += 1,
        |_: &Expr<()>| b += 1,
        |_: &Type<()>| c += 1,
    ));
    assert_eq!((a, b, c), (1, 1, 1));
}

// Struct passes: by `&mut`, boxed, tupled, and written against a level other than the entry's.

#[derive(Default)]
struct Counts {
    items: u32,
    types: u32,
}
top::impl_chain! { top; Counts;
    fn visit_type(&mut self, i) { self.types += 1; base::visit_type(self, i); }
}
impl<S> top::Visit<S> for Counts {
    fn visit_item(&mut self, i: &Item<S>) {
        self.items += 1;
        top::visit_item(self, i);
    }
}

#[test]
fn by_reference() {
    let mut c = Counts::default();
    sample().visit(&mut c);
    assert_eq!((c.items, c.types), (1, 1));
}

#[test]
fn boxed() {
    let mut b: Box<Counts> = Box::default();
    top::Visit::visit_item(&mut *b, &sample());
    assert_eq!((b.items, b.types), (1, 1));
}

#[test]
fn tuple_of_visitors() {
    let mut pair = (Counts::default(), Counts::default());
    sample().visit(&mut pair);
    assert_eq!((pair.0.items, pair.1.types), (1, 1));
}

// A pass that knows only the base still fires from the extended entry point.
#[derive(Default)]
struct BaseOnly {
    types: u32,
}
impl<S> base::Visit<S> for BaseOnly {
    fn visit_type(&mut self, i: &Type<S>) {
        self.types += 1;
        base::visit_type(self, i);
    }
}
impl<S> base::VisitMut<S> for BaseOnly {}
top::impl_chain! { top; BaseOnly; fn visit_type; }
impl<S> top::Visit<S> for BaseOnly {}

#[test]
fn a_base_level_pass_in_the_extended_walk() {
    let mut p = BaseOnly::default();
    sample().visit(&mut p);
    assert_eq!(p.types, 1);
}

#[test]
fn passes_from_different_levels_in_one_tuple() {
    let mut pair = (BaseOnly::default(), Counts::default());
    sample().visit(&mut pair);
    assert_eq!((pair.0.types, pair.1.items), (1, 1));
}

// A tuple is all closures or all visitors, because a tuple of closures is folded into one hook and
// walked once — a visitor cannot join that, since it descends on its own. To mix, turn the closure
// into a visitor first; the result is a tuple of visitors, which walks once per element.

#[test]
fn a_converted_closure_sits_in_a_tuple_of_visitors() {
    use top::IntoVisitor;
    let mut n = 0;
    let mut c = Counts::default();
    {
        let closure = (|_: &Type<()>| n += 1).into_visitor();
        sample().visit(&mut (closure, &mut c));
    }
    assert_eq!((n, c.items), (1, 1));
}

#[test]
fn three_passes_of_mixed_origin() {
    use top::IntoVisitor;
    let mut n = 0;
    let (mut a, mut b) = (Counts::default(), BaseOnly::default());
    {
        let closure = (|_: &Stmt<()>| n += 1).into_visitor();
        sample().visit(&mut (closure, &mut a, &mut b));
    }
    assert_eq!((n, a.items, b.types), (1, 1, 1));
}

// A tuple of closures is folded into one hook and walked once — the callbacks interleave per node
// rather than running one closure's whole traversal and then the next.
#[test]
fn a_tuple_of_closures_is_a_single_walk() {
    use core::cell::RefCell;
    let order = RefCell::new(String::new());
    sample().visit((
        |_: &Item<()>| order.borrow_mut().push('i'),
        |_: &Stmt<()>| order.borrow_mut().push('s'),
        |_: &Type<()>| order.borrow_mut().push('t'),
    ));
    assert_eq!(
        order.into_inner(),
        "ist",
        "one pass down the chain, not three"
    );
}
