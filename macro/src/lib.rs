//! Proc macros for the `visiting` crate: the `#[visiting]` attribute that declares a node, the
//! check it hands to `#[announce]`, and the `define_visit!` generator.
//!
//! Nothing here is called directly. `visiting` is re-exported as `visiting::visiting`, the check as
//! `visiting::_imp::visiting`, and the two
//! `__visitor_*` entry points are driven by the `visiting::visit::define_visit!` shim, which hands them the
//! crate path through `$crate` so the code they emit resolves wherever it lands.

use proc_macro::TokenStream as TokenStream1;
use proc_macro_error3::proc_macro_error;

mod absolute;
mod util;
mod visiting;
mod visitor;

fn random() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}

/// Declare a node: write its `#[announce]`, with the check below. See `visiting::visiting`.
#[proc_macro_error]
#[proc_macro_attribute]
pub fn visiting(args: TokenStream1, item: TokenStream1) -> TokenStream1 {
    crate::visiting::attribute(args.into(), item.into()).into()
}

/// The check `#[visiting]` hands to `#[announce]`: own the attributes that go inside the item.
/// Reached as `visiting::_imp::visiting`, so its name in announce's META is `visiting`.
#[proc_macro_error]
#[doc(hidden)]
#[proc_macro]
pub fn visiting_check(input: TokenStream1) -> TokenStream1 {
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
