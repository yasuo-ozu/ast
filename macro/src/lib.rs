//! Proc macros for the `ast` crate: the `Visiting` derive and the `define_visit!` generator.
//!
//! Nothing here is called directly. `Visiting` is re-exported as `visiting::visit::Visiting`, and the two
//! `__visitor_*` entry points are driven by the `visiting::visit::define_visit!` shim, which hands them the
//! crate path through `$crate` so the code they emit resolves wherever it lands.

use proc_macro::TokenStream as TokenStream1;
use proc_macro_error3::proc_macro_error;
use syn::*;

mod util;
mod visiting;
mod visitor;

fn random() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}

/// Derive `Visiting` — mark a type as a node, and record the metadata a visitor walks it by.
///
/// See `visiting::Visiting` for the attributes (`#[visiting]`, `#[seq]`, `#[opt]`, `#[skip]`).
#[proc_macro_error]
#[proc_macro_derive(Visiting, attributes(visiting, seq, opt, skip))]
pub fn derive_visiting(input: TokenStream1) -> TokenStream1 {
    let input: DeriveInput = parse_macro_input!(input);
    // Which crate the generated code names. `#[visiting(crate = ::my_crate)]` points it at a facade
    // that re-exports this crate, which is what a grammar depending only on that facade needs;
    // otherwise the impls read `::visiting::..`.
    let path = visiting::parse_crate_path(&input.attrs).unwrap_or_else(|| parse_quote!(::visiting));
    crate::visiting::derive_visiting(&input, random(), &path).into()
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
