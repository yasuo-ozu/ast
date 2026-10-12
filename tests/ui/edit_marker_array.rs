//! `#[seq]` on a fixed-size array — arrays traverse as sequences but have no `SeqView` impl (they can't
//! be structurally edited). This is a clean error, not a cryptic `[Leaf; N]: SeqView<Leaf>` trait error.
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
    pub items: [Leaf<S>; 3],
}

pub mod vis {
    visiting::define_visit!(crate::Wrap, crate::Leaf);
}

fn main() {
    let _ = PhantomData::<()>;
}
