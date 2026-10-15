//! Two sibling bases each contributing a type whose path ends in the same ident. Neither base has
//! ever seen the other, so only the visitor extending both can notice.
use core::marker::PhantomData;
use visiting::Visiting;

pub mod m1 {
    #[derive(visiting::Visiting)]
    pub enum Foo<S> {
        A(core::marker::PhantomData<S>),
    }
}
pub mod m2 {
    #[derive(visiting::Visiting)]
    pub enum Foo<S> {
        B(core::marker::PhantomData<S>),
    }
}

#[derive(Visiting)]
pub struct T<S> {
    pub p: PhantomData<S>,
}

pub mod v1 {
    visiting::define_visit!(crate::m1::Foo);
}
pub mod v2 {
    visiting::define_visit!(crate::m2::Foo);
}
pub mod w {
    visiting::define_visit!(crate::v1, crate::v2 => crate::T);
}

fn main() {}
