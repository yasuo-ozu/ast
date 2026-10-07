//! A tuple holds closures or visitors, never both: a tuple of closures is folded into one hook,
//! which only observes, and a visitor descends on its own. Convert the closure first if you need
//! both — `(closure.into_visitor(), &mut pass)`.
use ast::Ast;

#[derive(Ast)]
pub enum Expr {
    Lit(u32),
}

pub mod visit {
    ast::visitor!(crate::Expr);
}

#[derive(Default)]
struct Pass(u32);
impl visit::Visit for Pass {}
impl visit::VisitMut for Pass {}

fn main() {
    let mut n = 0;
    let mut p = Pass::default();
    Expr::Lit(1).visit((|_: &Expr| n += 1, &mut p));
}
