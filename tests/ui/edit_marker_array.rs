//! `#[seq]` on a fixed-size array — arrays traverse as sequences but have no `SeqView` impl (they can't
//! be structurally edited). This is a clean error, not a cryptic `[Leaf; N]: SeqView<Leaf>` trait error.
use core::marker::PhantomData;

#[announce::announce(visiting::visiting)]
pub enum Leaf<S> {
    A(::core::marker::PhantomData<S>),
}

#[announce::announce(visiting::visiting)]
#[visiting(crate::Leaf)]
pub struct Wrap<S> {
    #[seq]
    pub items: [crate::Leaf<S>; 3],
}

pub mod vis {
    visiting::define_visit!(crate::Wrap, crate::Leaf);
}

fn main() {
    let _ = PhantomData::<()>;
}
