//! `#[seq]` on a container-less field (a bare `Box<Leaf>`) — a marker needs a real collection. Clean
//! error, not a cryptic `Box<Leaf>: SeqView<Leaf>` trait error via the Box forwarder.
use core::marker::PhantomData;

#[announce::announce(visiting::visiting)]
pub enum Leaf<S> {
    A(::core::marker::PhantomData<S>),
}

#[announce::announce(visiting::visiting)]
#[visiting(crate::Leaf)]
pub struct Wrap<S> {
    #[seq]
    pub inner: ::std::boxed::Box<crate::Leaf<S>>,
}

pub mod vis {
    visiting::define_visit!(crate::Wrap, crate::Leaf);
}

fn main() {
    let _ = PhantomData::<()>;
}
