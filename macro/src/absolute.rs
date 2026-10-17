//! The paths of a node must be absolute.
//!
//! `#[visiting]` announces the node, and announce re-uses the definition where a visitor is built —
//! another module, another crate — so it accepts only paths that resolve from anywhere. It reports
//! the first one that does not, and a definition usually has several, so the attribute checks them
//! all here first, against the same rules:
//!
//! * a type path is rooted (`::..`, `crate::..`), a primitive, `Self`, one of the item's type
//!   parameters, or an associated type of `Self` or of a parameter (`T::Out`);
//! * a trait path in a bound is rooted, except `Sized`;
//! * a value path (an array length, a discriminant, a const argument) is rooted, `Self::..`, or one
//!   of the item's const parameters.
//!
//! Std's prelude names count as relative: the reader's prelude may differ. Attributes, visibilities
//! and macro invocations are not checked; announce does not check them either.

use proc_macro2::Ident;
use proc_macro_error3::emit_error;
use std::collections::HashSet;
use syn::visit::{self, Visit};
use syn::*;
use template_quote::quote;

const PRIMITIVES: [&str; 17] = [
    "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
    "i128", "isize", "f32", "f64",
];

/// Where std's commonly written names live, for the suggestion in the error.
const STD_PATHS: [(&str, &str); 22] = [
    ("Box", "::std::boxed::Box"),
    ("Vec", "::std::vec::Vec"),
    ("String", "::std::string::String"),
    ("Option", "::core::option::Option"),
    ("Result", "::core::result::Result"),
    ("PhantomData", "::core::marker::PhantomData"),
    ("Rc", "::std::rc::Rc"),
    ("Arc", "::std::sync::Arc"),
    ("Cell", "::core::cell::Cell"),
    ("RefCell", "::core::cell::RefCell"),
    ("VecDeque", "::std::collections::VecDeque"),
    ("HashMap", "::std::collections::HashMap"),
    ("HashSet", "::std::collections::HashSet"),
    ("BTreeMap", "::std::collections::BTreeMap"),
    ("BTreeSet", "::std::collections::BTreeSet"),
    ("Clone", "::core::clone::Clone"),
    ("Copy", "::core::marker::Copy"),
    ("Default", "::core::default::Default"),
    ("Debug", "::core::fmt::Debug"),
    ("PartialEq", "::core::cmp::PartialEq"),
    ("Eq", "::core::cmp::Eq"),
    ("Send", "::core::marker::Send"),
];

/// Report every path of `input` that announce would reject. `true` when there was none.
pub(crate) fn check(input: &DeriveInput) -> bool {
    let mut c = Check {
        types: HashSet::new(),
        consts: HashSet::new(),
        ok: true,
    };
    for p in &input.generics.params {
        match p {
            GenericParam::Type(t) => {
                c.types.insert(t.ident.clone());
            }
            GenericParam::Const(k) => {
                c.consts.insert(k.ident.clone());
            }
            GenericParam::Lifetime(_) => {}
        }
    }
    c.visit_derive_input(input);
    c.ok
}

struct Check {
    types: HashSet<Ident>,
    consts: HashSet<Ident>,
    ok: bool,
}

fn rooted(p: &Path) -> bool {
    p.leading_colon.is_some()
        || p.segments
            .first()
            .map_or(false, |s| s.ident == "crate" || s.ident == "$crate")
}

impl Check {
    fn report(&mut self, p: &Path, what: &str) {
        self.ok = false;
        let last = &p.segments.last().unwrap().ident;
        let hint = STD_PATHS
            .iter()
            .find(|(name, _)| p.segments.len() == 1 && last == name)
            .map_or_else(String::new, |(_, abs)| format!(" — here `{}`", abs));
        emit_error!(
            p,
            "{} `{}` is not an absolute path. `#[visiting]` announces the node, and the \
             definition is read again where a visitor is built, so write it as `crate::..`, \
             `::std::..`, `::core::..` or `::other_crate::..`{} (std's prelude names count as \
             relative)",
            what,
            quote!(#p).to_string().replace(' ', ""),
            hint,
        );
    }

    /// A type path with no qualified self.
    fn type_path(&mut self, p: &Path) {
        let first = &p.segments.first().unwrap().ident;
        let fine = rooted(p)
            || first == "Self"
            || self.types.contains(first)
            // syn reads a bare const argument, `Arr<N>`, as a type.
            || (p.segments.len() == 1 && self.consts.contains(first))
            || (p.segments.len() == 1
                && matches!(p.segments[0].arguments, PathArguments::None)
                && PRIMITIVES.iter().any(|t| first == t));
        if !fine {
            self.report(p, "type");
        }
    }
}

impl<'ast> Visit<'ast> for Check {
    fn visit_attribute(&mut self, _: &'ast Attribute) {}

    fn visit_visibility(&mut self, _: &'ast Visibility) {}

    fn visit_macro(&mut self, _: &'ast Macro) {}

    fn visit_type_path(&mut self, tp: &'ast TypePath) {
        match &tp.qself {
            // `<T as Trait>::Out`: the trait is written in the path's first `position` segments.
            Some(q) if q.position > 0 => {
                self.visit_type(&q.ty);
                if !rooted(&tp.path) {
                    self.report(&tp.path, "trait");
                }
            }
            Some(q) => self.visit_type(&q.ty),
            None => self.type_path(&tp.path),
        }
        for s in &tp.path.segments {
            self.visit_path_arguments(&s.arguments);
        }
    }

    fn visit_trait_bound(&mut self, b: &'ast TraitBound) {
        if !rooted(&b.path) && !b.path.is_ident("Sized") {
            self.report(&b.path, "trait");
        }
        if let Some(l) = &b.lifetimes {
            self.visit_bound_lifetimes(l);
        }
        for s in &b.path.segments {
            self.visit_path_arguments(&s.arguments);
        }
    }

    fn visit_expr_path(&mut self, ep: &'ast ExprPath) {
        let p = &ep.path;
        let first = &p.segments.first().unwrap().ident;
        let fine = ep.qself.is_some()
            || rooted(p)
            || first == "Self"
            || (p.segments.len() == 1 && self.consts.contains(first));
        if !fine {
            self.report(p, "value");
        }
        visit::visit_expr_path(self, ep);
    }

    // Visited for its arguments only: a path's head is judged by the type, bound or value around it.
    fn visit_path(&mut self, p: &'ast Path) {
        for s in &p.segments {
            self.visit_path_arguments(&s.arguments);
        }
    }
}
