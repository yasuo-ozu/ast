//! A union has no walkable fields, and nothing downstream handles one — so the derive says so,
//! rather than leaving `define_visit!` to report "no AST definitions resolved".

use visiting::Visiting;

#[derive(Visiting)]
pub union U {
    pub a: u32,
}

fn main() {}
