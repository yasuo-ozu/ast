//! `#[seq]` on a container-less field (a bare `Box<Leaf>`) — a marker needs a real collection. Clean
//! error, not a cryptic `Box<Leaf>: SeqView<Leaf>` trait error via the Box forwarder.
use core::marker::PhantomData;
use visiting::Visiting;

#[derive(Visiting)]
pub enum Leaf<S> {
    A(PhantomData<S>),
}

#[derive(Visiting)]
#[visiting(crate::Leaf)]
pub struct Wrap<S> {
    #[seq]
    pub inner: Box<Leaf<S>>,
}

pub mod vis {
    visiting::define_visit!(crate::Wrap, crate::Leaf);
}

fn main() {
    let _ = PhantomData::<()>;
}
