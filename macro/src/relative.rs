//! Carry a definition's relative paths through `announce`, which accepts absolute ones only.
//!
//! `#[derive(Visiting)]` announces the node, and announce checks that every path in the definition
//! resolves wherever the payload is used: `crate::..`, `::..` and primitives pass, a `Box<Expr>` or an
//! imported `Local` does not. A visitor never resolves those paths — it matches a field's head by its
//! segments and names nothing it reads from the payload — so the derive hands announce a copy in which
//! every other path is rooted at a name no crate has, `::__visiting_rel::..`, and the visitor removes
//! that root again from each definition it fetches. What the visitor reads is then the definition as
//! the derive cleaned it, as when the derive wrote its own trampoline.
//!
//! The root goes before the path's first segment, and a first segment that only means something in
//! first place (`crate`, `self`, `super`, `Self`) is renamed to `__visiting_<kw>` behind it. Attributes,
//! visibilities and macro invocations are left as they are: announce does not check them, and the
//! visitor reads the field markers by their bare names.

use proc_macro2::Span;
use syn::visit_mut::{self, VisitMut};
use syn::*;

const ROOT: &str = "__visiting_rel";
const KEYWORDS: [&str; 4] = ["crate", "self", "super", "Self"];

/// Root every relative path of `item` at `::__visiting_rel`.
pub(crate) fn mark(item: &mut DeriveInput) {
    Rewrite { path: mark_path }.visit_derive_input_mut(item);
}

/// Undo [`mark`] on a fetched definition. A definition nothing marked comes back unchanged.
pub(crate) fn unmark(item: &mut Item) {
    Rewrite { path: unmark_path }.visit_item_mut(item);
}

fn mark_path(p: &mut Path) {
    if p.leading_colon.is_some() {
        return;
    }
    if let Some(first) = p.segments.first_mut() {
        if KEYWORDS.iter().any(|k| first.ident == k) {
            first.ident = Ident::new(&format!("__visiting_{}", first.ident), first.ident.span());
        }
    }
    p.segments
        .insert(0, Ident::new(ROOT, Span::call_site()).into());
    p.leading_colon = Some(Default::default());
}

fn unmark_path(p: &mut Path) {
    let marked = p.leading_colon.is_some()
        && p.segments.len() > 1
        && p.segments.first().map_or(false, |s| s.ident == ROOT);
    if !marked {
        return;
    }
    let rest = std::mem::take(&mut p.segments).into_iter().skip(1);
    p.segments = rest.collect();
    p.leading_colon = None;
    let first = p.segments.first_mut().unwrap();
    let name = first.ident.to_string();
    if let Some(kw) = name.strip_prefix("__visiting_") {
        if KEYWORDS.contains(&kw) {
            first.ident = Ident::new(kw, first.ident.span());
        }
    }
}

/// Applies `path` to every path of an item that is not part of an attribute, a visibility or a
/// macro invocation.
struct Rewrite {
    path: fn(&mut Path),
}

impl Rewrite {
    /// A path behind a qualified self, `<T as Trait>::Out`: the segments before `position` belong to
    /// the trait, so the root goes in front of them and `position` follows it. `<T>::Out` (position
    /// 0) has no trait, and its segments are associated items of `T` — rooting them would turn them
    /// into a trait — so only `T` is rewritten.
    fn qualified(&mut self, qself: &mut Option<QSelf>, path: &mut Path) {
        match qself {
            Some(q) => {
                self.visit_type_mut(&mut q.ty);
                if q.position == 0 {
                    visit_mut::visit_path_mut(self, path);
                } else {
                    let before = path.segments.len();
                    self.visit_path_mut(path);
                    q.position = q.position + path.segments.len() - before;
                }
            }
            None => self.visit_path_mut(path),
        }
    }
}

impl VisitMut for Rewrite {
    fn visit_attribute_mut(&mut self, _: &mut Attribute) {}

    fn visit_visibility_mut(&mut self, _: &mut Visibility) {}

    fn visit_macro_mut(&mut self, _: &mut Macro) {}

    fn visit_path_mut(&mut self, p: &mut Path) {
        visit_mut::visit_path_mut(self, p);
        (self.path)(p);
    }

    fn visit_type_path_mut(&mut self, tp: &mut TypePath) {
        self.qualified(&mut tp.qself, &mut tp.path);
    }

    fn visit_expr_path_mut(&mut self, ep: &mut ExprPath) {
        self.qualified(&mut ep.qself, &mut ep.path);
    }
}
