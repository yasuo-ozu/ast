//! Proc macros for the `ast` crate: the `Visiting` derive and the `define_visit!` generator.
//!
//! Nothing here is called directly. `Visiting` is re-exported as `visiting::visit::Visiting`, and the two
//! `__visitor_*` entry points are driven by the `visiting::visit::define_visit!` shim, which hands them the
//! crate path through `$crate` so the code they emit resolves wherever it lands.

use proc_macro::TokenStream as TokenStream1;
use proc_macro_error3::{abort, proc_macro_error};
use syn::*;

mod ast;
mod util;
mod visitor;

fn random() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}

/// The crate path the generated code names, from `#[visiting(<path>)]` on the type, or `::visiting`.
///
/// A facade that re-exports this crate can be named instead — `#[visiting(::my_crate)]` makes the impls read
/// `::my_crate::visit::..`, which is what a grammar depending only on that facade needs.
fn crate_path(attrs: &[Attribute]) -> Path {
    for attr in attrs {
        if attr.path().is_ident("visiting") {
            if let Meta::List(MetaList { tokens, .. }) = &attr.meta {
                if let Ok(path) = parse2::<Path>(tokens.clone()) {
                    return path;
                }
            }
            abort!(attr, "should be formatted as #[visiting(<path>)]");
        }
    }
    parse_quote!(::visiting)
}

/// Derive `Visiting` — mark a type as a node, and record the metadata a visitor walks it by.
///
/// See `visiting::Visiting` for the attributes (`#[subast]`, `#[seq]`, `#[opt]`, `#[skip]`).
#[proc_macro_error]
#[proc_macro_derive(Visiting, attributes(visiting, subast, seq, opt, skip))]
pub fn ast_derive(input: TokenStream1) -> TokenStream1 {
    let input: DeriveInput = parse_macro_input!(input);
    let path = crate_path(&input.attrs);
    ast::derive_ast(&input, random(), &path).into()
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
