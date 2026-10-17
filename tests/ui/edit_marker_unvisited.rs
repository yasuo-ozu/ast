//! `#[seq]` on a field whose element type is not a visited type — the marker would route nowhere and
//! silently no-op. Clean error instead of silent drop (previously surfaced only as a later E0407).
use core::marker::PhantomData;

#[visiting::visiting]
pub enum Leaf<S> {
    A(::core::marker::PhantomData<S>),
}

#[visiting::visiting]
#[visiting(crate::Leaf)]
pub struct Wrap<S> {
    #[seq]
    pub items: ::std::vec::Vec<::std::string::String>, // `String` is not a visited AST type
    pub leaf: crate::Leaf<S>,
}

pub mod vis {
    visiting::define_visit!(crate::Wrap, crate::Leaf);
}

fn main() {
    let _ = PhantomData::<()>;
}
