//! Proc macros for the `visiting` crate: the `visiting` check that declares a node, and the
//! `define_visit!` generator.
//!
//! Nothing here is called directly. `visiting` is re-exported as `visiting::visiting`, and the two
//! `__visitor_*` entry points are driven by the `visiting::visit::define_visit!` shim, which hands them the
//! crate path through `$crate` so the code they emit resolves wherever it lands.

use proc_macro::TokenStream as TokenStream1;
use proc_macro_error3::proc_macro_error;

mod util;
mod visiting;
mod visitor;

fn random() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}

/// Declare a node from an `#[announce]` definition, and own the attributes that go inside it.
/// See `visiting::visiting`.
#[proc_macro_error]
#[proc_macro]
pub fn visiting(input: TokenStream1) -> TokenStream1 {
    announce_core::helper::check(input.into(), &["visiting", "seq", "opt", "skip"], |input| {
        Ok(crate::visiting::check(input))
    })
    .into()
}

#[proc_macro_error]
#[doc(hidden)]
#[proc_macro]
pub fn __visitor_entry(input: TokenStream1) -> TokenStream1 {
    visitor::entry(input.into(), random()).into()
}

#[proc_macro_error]
#[doc(hidden)]
#[proc_macro]
pub fn __visitor_build(input: TokenStream1) -> TokenStream1 {
    visitor::build(input.into()).into()
}
