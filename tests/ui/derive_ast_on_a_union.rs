//! A union has no walkable fields, and nothing downstream handles one — so the derive says so,
//! rather than leaving `visitor!` to report "no AST definitions resolved".

use ast::Ast;

#[derive(Ast)]
pub union U {
    pub a: u32,
}

fn main() {}
