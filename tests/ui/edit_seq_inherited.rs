//! A `#[seq]`/`#[opt]` field can only view a type the visitor *targets* — its `visit_<t>_seq`/`_opt` is
//! emitted only for the visitor's own listed types. A marker on a field that points at an INHERITED base
//! type (`Item`, owned by `base`) is a clean error, not a cryptic "no method `visit_item_seq`" downstream.

use core::marker::PhantomData;
use visiting::Visiting;

#[derive(Visiting)]
pub enum Item<S> {
    Leaf(::core::marker::PhantomData<S>),
}

#[derive(Visiting)]
#[visiting(crate::Item)]
pub struct List<S> {
    #[seq]
    pub items: ::std::vec::Vec<crate::Item<S>>, // `Item` is inherited from `base`, not targeted by `ext`
}

pub mod base {
    visiting::define_visit!(crate::Item);
}

pub mod ext {
    visiting::define_visit!(crate::base => crate::List);
}

fn main() {
    let _ = PhantomData::<()>;
}
