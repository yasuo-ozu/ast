//! The same type listed twice would emit every item it names twice. Saying so here beats the
//! `E0428` cascade about `__ast_ty_*`/`visit_*` — names the user never wrote.

use ast::Ast;

#[derive(Ast)]
pub enum Item {
    Leaf(u32),
}

pub mod visit {
    ast::visitor!(crate::Item, crate::Item);
}

fn main() {}
