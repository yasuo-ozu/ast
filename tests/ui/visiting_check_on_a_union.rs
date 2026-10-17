//! A union has no walkable fields, and nothing downstream handles one — so the check says so,
//! rather than leaving `define_visit!` to report "no AST definitions resolved".

#[announce::announce(visiting::visiting)]
pub union U {
    pub a: u32,
}

fn main() {}
