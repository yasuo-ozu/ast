//! A node is announced and read again where a visitor is built, so its paths are absolute.
//! Every relative one is reported at once — a std name with its absolute form — not only the first.

use visiting::visiting;

pub mod inner {
    pub struct Local;
}
use inner::Local;

pub trait Bound {}
pub const N: usize = 2;
pub struct Bare;

#[visiting]
pub struct Node<T: Bound> {
    // Fine as they are: a primitive, a parameter, rooted paths.
    pub n: u32,
    pub t: T,
    pub ok: ::std::vec::Vec<crate::Bare>,
    // Relative: a prelude name and the type inside it, an import, a bound, a const.
    pub boxed: Box<Node<T>>,
    pub local: Local,
    pub arr: [u8; N],
}

fn main() {}
