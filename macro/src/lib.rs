//! Proc macros for the `ast` crate: the `Ast` derive and the `visitor!` generator.
//!
//! Nothing here is called directly. `Ast` is re-exported as `ast::visit::Ast`, and the two
//! `__visitor_*` entry points are driven by the `ast::visit::visitor!` shim, which hands them the
//! crate path through `$crate` so the code they emit resolves wherever it lands.

use proc_macro::TokenStream as TokenStream1;
use proc_macro_error::{abort, proc_macro_error};
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

/// The crate path the generated code names, from `#[ast(<path>)]` on the type, or `::ast`.
///
/// A facade that re-exports this crate can be named instead — `#[ast(::my_crate)]` makes the impls read
/// `::my_crate::visit::..`, which is what a grammar depending only on that facade needs.
fn crate_path(attrs: &[Attribute]) -> Path {
    for attr in attrs {
        if attr.path().is_ident("ast") {
            if let Meta::List(MetaList { tokens, .. }) = &attr.meta {
                if let Ok(path) = parse2::<Path>(tokens.clone()) {
                    return path;
                }
            }
            abort!(attr, "should be formatted as #[ast(<path>)]");
        }
    }
    parse_quote!(::ast)
}

/// Derive `Ast` — mark a type as a node, and record the metadata a visitor walks it by.
///
/// See `ast::visit::Ast` for the attributes (`#[subast]`, `#[seq]`, `#[opt]`, `#[skip]`).
#[proc_macro_error]
#[proc_macro_derive(Ast, attributes(ast, subast, seq, opt, skip))]
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
