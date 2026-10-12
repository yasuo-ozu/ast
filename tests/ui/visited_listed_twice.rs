//! The same type listed twice would emit every item it names twice. Saying so here beats the
//! `E0428` cascade about `__visiting_ty_*`/`visit_*` — names the user never wrote.

use visiting::Visiting;

#[derive(Visiting)]
pub enum Item {
    Leaf(u32),
}

pub mod visit {
    visiting::define_visit!(crate::Item, crate::Item);
}

fn main() {}
