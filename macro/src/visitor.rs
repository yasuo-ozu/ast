use crate::util::{
    angle, for_each_field_type, gargs, gparams, indicator, innermost_acc, item_generics,
    item_ident, param_name, param_name_ident, param_use, path_may_denote, peel, strip_bounds,
    to_snake, Container, Head, LayerKind, Side,
};
use proc_macro2::{Span, TokenStream};
use proc_macro_error3::abort;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::*;
use template_quote::quote;

mod build_input;
mod discover;
mod entry;
mod lower;
mod model;
mod params;
mod side;
use build_input::*;
use discover::*;
use entry::*;
use lower::*;
use model::*;
use params::*;
use side::*;

// Preserve the public entry points at `crate::visitor::{entry,build}` for `lib.rs`.
pub(crate) use build_input::build;
pub(crate) use entry::entry;
/// Does any visited type's field reference another visited type while filling a **non-shared** generic
/// param position with something other than that param verbatim (e.g. `Box<Stmt<S, u8>>` where `Stmt`'s
/// `T` is non-shared)? Such a *concrete fill* can't be expressed with the union-of-params trait model
/// (the trait would fix `T`, but the cross-edge needs a specific `T`), so the visitor must instead make
/// the non-shared params **per-method generics** (`visit_stmt<T>`). Closures can't be `for<T>` generic,
/// so that mode is struct-only. The common case (no concrete fill) keeps the union model + closures.
fn has_concrete_fill(targets: &[&DoneType], shared: &HashSet<String>) -> bool {
    // Each visited type's own params, in declaration order (lifetimes precede types/consts, matching how
    // generic *arguments* must be ordered — so args zip directly onto params).
    let params_of: HashMap<String, Vec<GenericParam>> = targets
        .iter()
        .filter_map(|d| {
            let id = item_ident(&d.def)?;
            Some((id.to_string(), gparams(item_generics(&d.def)?)))
        })
        .collect();

    fn ty_fills(
        ty: &Type,
        params_of: &HashMap<String, Vec<GenericParam>>,
        shared: &HashSet<String>,
    ) -> bool {
        match ty {
            Type::Path(tp) => {
                for seg in &tp.path.segments {
                    if let PathArguments::AngleBracketed(ab) = &seg.arguments {
                        // Zip the actual args onto the referenced type's declared params (same order).
                        // Only a non-shared TYPE or CONST param filled with a non-identity arg forces
                        // method-generic mode — a lifetime fill (`Stmt<'static, S>`) is fine in the
                        // union model via subtyping, so it does NOT trigger it.
                        if let Some(decl) = params_of.get(&seg.ident.to_string()) {
                            for (param, arg) in decl.iter().zip(ab.args.iter()) {
                                match (param, arg) {
                                    (GenericParam::Type(tp_), GenericArgument::Type(at)) => {
                                        let pname = tp_.ident.to_string();
                                        let bare = matches!(at, Type::Path(p)
                                            if p.qself.is_none() && p.path.is_ident(&pname));
                                        if !shared.contains(&pname) && !bare {
                                            return true;
                                        }
                                    }
                                    (GenericParam::Const(cp), GenericArgument::Const(ce)) => {
                                        let pname = cp.ident.to_string();
                                        let bare = matches!(ce, syn::Expr::Path(p)
                                            if p.qself.is_none() && p.path.is_ident(&pname));
                                        if !shared.contains(&pname) && !bare {
                                            return true;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        for arg in &ab.args {
                            if let GenericArgument::Type(at) = arg {
                                if ty_fills(at, params_of, shared) {
                                    return true;
                                }
                            }
                        }
                    }
                }
                false
            }
            Type::Reference(r) => ty_fills(&r.elem, params_of, shared),
            Type::Slice(s) => ty_fills(&s.elem, params_of, shared),
            Type::Array(a) => ty_fills(&a.elem, params_of, shared),
            Type::Paren(p) => ty_fills(&p.elem, params_of, shared),
            Type::Group(g) => ty_fills(&g.elem, params_of, shared),
            Type::Tuple(t) => t.elems.iter().any(|e| ty_fills(e, params_of, shared)),
            _ => false,
        }
    }

    targets.iter().any(|d| {
        let mut found = false;
        for_each_field_type(&d.def, &mut |t| found |= ty_fills(t, &params_of, shared));
        found
    })
}

/// The per-module tag type threaded through [`crate::visit::Walk`] as its first parameter, so two
/// `define_visit!` modules over the same node type do not write conflicting impls.
pub(crate) fn walk_tag() -> Ident {
    Ident::new("__VisitingWalkTag", Span::call_site())
}

/// The tag's generic params: the union's lifetimes and type params, but **not** its const params — a
/// const of arbitrary type cannot be mentioned in a struct field, and a node that uses one has it
/// constrained by its own self type anyway.
pub(crate) fn walk_tag_params(g_params: &[GenericParam]) -> Vec<GenericParam> {
    g_params
        .iter()
        .filter(|p| !matches!(p, GenericParam::Const(_)))
        .cloned()
        .collect()
}

/// `<'a, S>` for the tag, matching [`walk_tag_params`].
pub(crate) fn walk_tag_use(g_params: &[GenericParam]) -> TokenStream {
    let args: Vec<TokenStream> = walk_tag_params(g_params).iter().map(param_use).collect();
    angle(&args)
}

/// The tag's `PhantomData` payload, so none of its params is left unused.
pub(crate) fn walk_tag_phantom(g_params: &[GenericParam]) -> TokenStream {
    let parts: Vec<TokenStream> = walk_tag_params(g_params)
        .iter()
        .map(|p| match p {
            GenericParam::Lifetime(l) => {
                let lt = &l.lifetime;
                quote!(& #lt ())
            }
            GenericParam::Type(t) => {
                let id = &t.ident;
                quote!(#id)
            }
            GenericParam::Const(_) => unreachable!("const params are filtered out"),
        })
        .collect();
    quote!( ::core::marker::PhantomData<( #(#parts,)* )> )
}

/// A drilled intermediate — reached through `#[visiting]` but not listed in `define_visit!(..)`, so it has
/// no visit method — gets a `Walk` impl of its own instead of being destructured inline at every use
/// site. Unconditional, like a node's, so a `#[visiting]` cycle through one terminates.
fn intermediate_impls(
    st: &BuildInput,
    m: &Model,
    lowers: &[Lower; 2],
    union_where: &[WherePredicate],
) -> Vec<TokenStream> {
    st.done
        .iter()
        .filter(|d| item_ident(&d.def).map_or(false, |id| !m.method_set.contains(&id.to_string())))
        .flat_map(|d| {
            let own_use = angle(&gargs(item_generics(&d.def).unwrap()));
            let mut preds = union_where.to_vec();
            preds.extend(item_where_preds(&d.def));
            let where_cl = where_clause(&preds);
            // The trait's params, plus any this type declares that they do not already cover.
            let known: HashSet<String> = m.g_params.iter().map(param_name).collect();
            let mut params = m.g_params.clone();
            params.extend(
                gparams(item_generics(&d.def).unwrap())
                    .into_iter()
                    .filter(|p| !known.contains(&param_name(p))),
            );
            sort_lifetimes_first(&mut params);
            let (path, tag, targs) = (&d.path, walk_tag(), walk_tag_use(&m.g_params));
            let g_use = &m.g_use;
            let node = Node {
                def: &d.def,
                subast: &d.subast,
                path,
            };
            Side::both().map(|side| {
                let mut stack = Vec::new();
                let body = lowers[side.index()].destructure(node, &quote!(self), 0, &mut stack);
                let (tr, f, recv) = (side.walk_trait(), side.walk_fn(), side.recv());
                quote! {
                    impl< #(#params,)* __VisitingW: #{side.visit_trait()} #g_use #(if !m.sized_self) { + ?Sized } >
                        __visiting_rt::walk::#tr< #tag #targs, __visiting_rt::walk::Here, __VisitingW >
                        for #path #own_use #where_cl
                    {
                        fn #f(#recv, this: &mut __VisitingW) {
                            let _ = &this;
                            #body
                        }
                    }
                }
            })
        })
        .collect()
}

/// A `Walk` impl for each *inherited* head a field reached. That type lives in a base module, which
/// wrote its impl against the *base's* tag; this module needs one against its own, forwarding to the
/// inherited method through the supertrait.
fn inherited_impls(
    m: &Model,
    heads: &[(String, TokenStream, Ident)],
    union_where: &[WherePredicate],
) -> Vec<TokenStream> {
    let (tag, targs, uw) = (
        walk_tag(),
        walk_tag_use(&m.g_params),
        where_clause(union_where),
    );
    let (g_params, g_use) = (&m.g_params, &m.g_use);
    heads
        .iter()
        .flat_map(|(_, ty, id)| {
            Side::both().map(|side| {
                let (tr, f, recv, vt) = (
                    side.walk_trait(),
                    side.walk_fn(),
                    side.recv(),
                    side.visit_trait(),
                );
                let m_name = side.method(id);
                quote! {
                    impl< #(#g_params,)* __VisitingW: #vt #g_use #(if !m.sized_self) { + ?Sized } >
                        __visiting_rt::walk::#tr< #tag #targs, __visiting_rt::walk::Here, __VisitingW > for #ty #uw
                    {
                        fn #f(#recv, v: &mut __VisitingW) {
                            // Plain method syntax: an inherited `visit_*` is reached through the
                            // supertrait, so it is not a member of this module's own trait.
                            v.#m_name(self)
                        }
                    }
                }
            })
        })
        .collect()
}

/// A `#[seq]`/`#[opt]` field can only view a type this visitor *targets* — its `visit_*_seq`/`_opt`
/// is emitted only for the listed types. A marker pointing at an **inherited** base type would make
/// the descent call a `visit_<t>_seq` that lives nowhere, a cryptic E0599 in generated code.
fn check_edit_markers(
    seq_used: &HashSet<String>,
    opt_used: &HashSet<String>,
    visited: &HashSet<String>,
) {
    if let Some(t) = seq_used
        .iter()
        .chain(opt_used.iter())
        .find(|t| !visited.contains(*t))
    {
        abort!(
            Span::call_site(),
            "a `#[seq]`/`#[opt]` field views the inherited type `{}`; container-edit views are not \
             generated for inherited types (the `visit_{}` method would have nowhere to live). Drop the \
             marker — the field is still traversed, calling the inherited per-node visit for each element.",
            t,
            to_snake(&Ident::new(t, Span::call_site()))
        );
    }
}

fn generate_module(st: &BuildInput) -> TokenStream {
    check_last_segment_collisions(&st.visited);
    let m = Model::build(st);

    // Container-edit usage, recorded by the mut walk below and consumed by `gen_side` to decide which
    // `visit_<t>_seq` / `visit_<t>_opt` to emit. Shared by both `Lower`s (only the mut one records).
    let seq_used: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    let opt_used: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    // The heads this module emits `Walk` impls for. An inherited head is in `method_set` but not here.
    let target_set: HashSet<String> = m
        .targets
        .iter()
        .filter_map(|d| item_ident(&d.def).map(|i| i.to_string()))
        .chain(
            st.done
                .iter()
                .filter_map(|d| item_ident(&d.def).map(|i| i.to_string()))
                .filter(|n| !m.method_set.contains(n)),
        )
        .collect();
    let inherited_heads: RefCell<Vec<(String, TokenStream, Ident)>> = RefCell::new(Vec::new());
    let walk_tag_args = walk_tag_use(&m.g_params);
    let lowers = Side::both().map(|side| Lower {
        method_set: &m.method_set,
        done_by_path: &m.done_by_path,
        side,
        seq_used: &seq_used,
        opt_used: &opt_used,
        walk_tag_args: &walk_tag_args,
        target_set: &target_set,
        inherited_heads: &inherited_heads,
        reachable: &m.reachable,
        reachable_keys: &m.reachable_keys,
    });
    let [lower, lower_mut] = &lowers;

    let vtypes: Vec<VType> = m
        .targets
        .iter()
        .map(|d| {
            let def = &d.def;
            let ident = item_ident(def).unwrap().clone();
            let own_params = gparams(item_generics(def).unwrap());
            let own_use = angle(&gargs(item_generics(def).unwrap()));
            let own_where = item_where_preds(def);
            // In method-mode, this type's non-shared params become method generics; in union mode the
            // trait already carries every param, so none.
            let method_params: Vec<GenericParam> = if m.method_mode {
                own_params
                    .iter()
                    .filter(|p| !m.shared_names.contains(&param_name(p)))
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };
            let scrut_path: &Path = m
                .path_of
                .get(&ident.to_string())
                .copied()
                .unwrap_or(&d.path);
            let path_tokens = quote!(#scrut_path);
            let node = Node {
                def,
                subast: &d.subast,
                path: scrut_path,
            };
            let mut stack = Vec::new();
            let body = lower.destructure(node, &quote!(i), 0, &mut stack);
            let mut stack = Vec::new();
            let body_mut = lower_mut.destructure(node, &quote!(i), 0, &mut stack);
            VType {
                ident,
                path: path_tokens,
                own_params,
                own_use,
                own_where,
                method_params,
                local: path_is_crate_local(scrut_path),
                body,
                body_mut,
            }
        })
        .collect();

    // The union of every visited type's `where`-predicates (deduped by rendered text — identical
    // predicates from two types are harmless but noisy), applied (as `uw`) to each generated item
    // quantified over the param union so a `enum Expr<S> where S: Bound { .. }` stays well-formed there.
    // In method-mode the *non-shared* params are method generics, not trait params, so a bound on one
    // would reference an undeclared param at the trait level — drop it here; `gen_side` re-attaches it
    // to the per-type `visit_*` method + free fn that actually carries the param.
    let mut seen_pred: HashSet<String> = HashSet::new();
    let union_where: Vec<WherePredicate> = vtypes
        .iter()
        .flat_map(|vt| vt.own_where.iter().cloned())
        .filter(|p| {
            !m.method_mode
                || where_pred_param(p)
                    .map_or(true, |id| !m.unshared_names.contains(&id.to_string()))
        })
        .filter(|p| seen_pred.insert(quote!(#p).to_string()))
        .collect();

    let intermediates = intermediate_impls(st, &m, &lowers, &union_where);
    let inherited = inherited_impls(&m, &inherited_heads.borrow(), &union_where);

    let seq_used = seq_used.into_inner();
    let opt_used = opt_used.into_inner();
    check_edit_markers(&seq_used, &opt_used, &m.visited);

    let [shared, mutable] =
        Side::both().map(|side| gen_side(side, &m, &vtypes, &union_where, &seq_used, &opt_used));

    // Every visitor module exports its full visited-type set (idents), its generic-param union
    // (`@bg`), and its full ancestor chain (`@an`) so another visitor can inherit it (transitively).
    let anc_export = emit_ancestors(&m.chain);
    let own: Vec<Decl> = vtypes
        .iter()
        .map(|t| {
            // A method-generic target is exported by name only. Its `visit_*` is quantified per call,
            // so neither a closure hook nor an `impl_chain!` arm can write it — the same reason this
            // mode has no closure machinery of its own — and its arguments name parameters the
            // extender does not have. An extender still inherits the method through the supertrait
            // and overrides it by writing the impl out.
            let method_generic = !t.method_params.is_empty();
            Decl {
                ident: t.ident.clone(),
                args: if method_generic {
                    TokenStream::new()
                } else {
                    t.own_use.clone()
                },
                method_generic,
            }
        })
        .collect();
    let visited_macro = emit_visited_macro(st, &m.g_params, anc_export, &own, m.sized_self);
    let impl_chain = emit_impl_chain(&st.nonce, &m.ancestors);

    // The relay: one module per base, naming that base's traits plus the base's own relays, so an
    // extender reaches any ancestor as `<base>::__visiting_base_<k>::..` (see `relay_path`). Indexed
    // because a visitor may extend several bases, which makes the chain a tree of hops rather than a
    // depth.
    //
    // A module, not `pub use #b as __visiting_base_0`: re-exporting a *module* that is private is
    // E0365, and a private base module is exactly the case this exists for. Re-exporting the public
    // items out of it is allowed, so the relay forwards those instead.
    //
    // The aliases sit beside the relay rather than inside it because `#b` is written relative to
    // *this* module: a `super::`-relative path, or a 2018 uniform path through a local `use`, would
    // mean something else one level down.
    let base_relay_exports: Vec<TokenStream> = st
        .bases
        .iter()
        .enumerate()
        .map(|(k, base)| {
            let (b, hop) = (&base.path, base_relay_name(k));
            let id = |s: &str| Ident::new(s, Span::call_site());
            let (v, vm) = (
                id(&format!("__visiting_relay_{k}_Visit")),
                id(&format!("__visiting_relay_{k}_VisitMut")),
            );
            // This base's own hops, read back off the chain: an ancestor that came through base `k`
            // carries `hop` first and that base's own hop name second. One `pub use` per distinct
            // one, so the walk continues through it.
            let mut seen = HashSet::new();
            let onward_hops: Vec<Ident> = m
                .chain
                .iter()
                .filter(|a| a.relay.len() >= 2 && a.relay[0] == hop)
                .map(|a| a.relay[1].clone())
                .filter(|h| seen.insert(h.to_string()))
                .collect();
            let onward_alias: Vec<Ident> = onward_hops
                .iter()
                .map(|h| id(&format!("__visiting_relay_{k}_next_{h}")))
                .collect();
            // The base's free `visit_*` functions travel with its traits: an override of one of its
            // methods ends by calling the matching free fn to keep descending, and through a relay
            // path that call has to resolve too. Their names are the base's own targets — `@own`.
            let (fn_alias, fn_name): (Vec<Ident>, Vec<Ident>) = base
                .own
                .iter()
                .flat_map(|d| Side::both().map(|side| side.method(&d.ident)))
                .chain(
                    base.own
                        .iter()
                        .map(|d| id(&format!("__visiting_ty_{}", d.ident))),
                )
                .map(|f| (id(&format!("__visiting_relay_{k}_fn_{f}")), f))
                .unzip();
            quote! {
                #[doc(hidden)]
                pub use #b::{Visit as #v, VisitMut as #vm};
                #(
                    #[doc(hidden)]
                    #[allow(unused_imports)]
                    pub use #b::#fn_name as #fn_alias;
                )*
                #(
                    #[doc(hidden)]
                    pub use #b::#onward_hops as #onward_alias;
                )*

                #[doc(hidden)]
                pub mod #hop {
                    pub use super::{#v as Visit, #vm as VisitMut};
                    #( pub use super::#fn_alias as #fn_name; )*
                    #( pub use super::#onward_alias as #onward_hops; )*
                }
            }
        })
        .collect();

    // One alias per target, taking exactly that type's own parameters. An extender spells an
    // inherited type through the relay (`<module>::__visiting_base_0::__visiting_ty_Expr<S>`) rather than by
    // its own path: the path is `crate::`-rooted by convention, and `crate` inside a `macro_rules`
    // body means the crate that *invokes* it.
    let ty_aliases: Vec<TokenStream> = vtypes
        .iter()
        .map(|t| {
            let (ident, path, own) = (&t.ident, &t.path, &t.own_use);
            let alias = Ident::new(&format!("__visiting_ty_{ident}"), Span::call_site());
            let bare: Vec<GenericParam> = t.own_params.iter().cloned().map(strip_bounds).collect();
            quote! {
                #[doc(hidden)]
                #[allow(dead_code)]
                pub type #alias #{angle(&bare)} = #path #own;
            }
        })
        .collect();

    // Items are emitted directly into the enclosing module (where `define_visit!(...)` was invoked).
    quote! {
        #visited_macro
        #(for e in &base_relay_exports) { #e }
        #impl_chain
        #(for a in &ty_aliases) { #a }

        // Bring every ancestor's traits in scope so the generated `Driver` impls / method calls
        // resolve (transitive supertraits included).
        #(for a in &m.ancestors) {
            #[allow(unused_imports)]
            use #{&a.path}::{Visit as _, VisitMut as _};
        }



        // This module's tag, the first parameter of every `Walk` impl it writes, so two visitors
        // over the same node type do not collide. Private: nothing outside the module names it, and
        // a `use path::to::visit::*;` should not pick it up.
        #[doc(hidden)]
        struct #{walk_tag()} #{angle(&walk_tag_params(&m.g_params))} ( #{walk_tag_phantom(&m.g_params)} );

        #(for imp in &intermediates) { #imp }
        #(for imp in &inherited) { #imp }

        #shared
        #mutable
    }
}
