//! Two different types whose paths end in the same ident, each declared by a different visitor in
//! one inheritance chain. Neither the per-`define_visit!` check nor the `#[visiting]` ones see them
//! together, so this is where the chain-wide check has to catch it.
pub mod m1 { #[announce::announce(visiting::visiting)] #[derive(Debug)] pub enum Foo<S> { A(::core::marker::PhantomData<S>) } }
pub mod m2 { #[announce::announce(visiting::visiting)] #[derive(Debug)] pub enum Foo<S> { B(::core::marker::PhantomData<S>) } }
#[announce::announce(visiting::visiting)]
#[derive(Debug)]
pub struct Root<S> { pub p: ::core::marker::PhantomData<S> }
pub mod v1 { visiting::define_visit!(crate::m1::Foo); }
pub mod v2 { visiting::define_visit!(crate::v1 => crate::m2::Foo); }
pub mod v3 { visiting::define_visit!(crate::v2 => crate::Root); }

fn main() {}
