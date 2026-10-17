//! Where the check's own attributes may be written. A misplaced one is worth an error rather than
//! silence: an ignored `#[seq]` is a field walked the plain way, which no test notices, and a
//! `#[visiting(..)]` on a field declares nothing at all.

#[visiting::visiting]
pub struct Leaf {
    pub n: u32,
}

#[visiting::visiting]
#[seq]
pub struct OnTheType {
    pub many: ::std::vec::Vec<crate::Leaf>,
}

#[visiting::visiting]
pub struct OnAField {
    #[visiting(crate::Leaf)]
    pub one: crate::Leaf,
    #[seq(crate::Leaf)]
    pub many: ::std::vec::Vec<crate::Leaf>,
}

#[visiting::visiting]
#[visiting(crate::Leaf)]
pub enum OnAVariant {
    #[opt]
    A(::core::option::Option<crate::Leaf>),
}

fn main() {}
