use super::*;

/// Generate every item for one mutability "side" (`Visit`/`VisitMut`, etc.).
pub(crate) fn gen_side(
    side: Side,
    m: &Model,
    vtypes: &[VType],
    union_where: &[WherePredicate],
    // Which visited types some AST holds Vec-like / Option-like, and so get a `visit_<t>_seq` /
    // `visit_<t>_opt` container-edit view.
    seq_used: &HashSet<String>,
    opt_used: &HashSet<String>,
) -> TokenStream {
    let Model {
        g_params,
        g_args,
        g_def,
        g_use,
        base,
        base_g_use,
        ancestors,
        ..
    } = m;
    // Heterogeneous (method-generic) mode: a non-shared param is concrete-filled in a cross-edge, so
    // each `visit_*` carries its type's non-shared params as method generics. A closure can't be
    // `for<T>` generic, so the closure machinery (`&mut V` blanket / `Driver`/`Hook`/`Chain`/
    // `IntoVisitor`) is omitted and the inherent `.visit()` takes `&mut impl Visit` directly.
    let struct_only = m.method_mode;
    // The union predicates one ancestor's items can state: those about parameters that ancestor has.
    // The rest name parameters those items are not generic in at all, so stating them there is
    // `E0425: cannot find type T` — and they are not this ancestor's to discharge anyway, since its
    // trait never mentions them. Naming `base::Visit<S>` where the base keyed it on `S: Bound` asks
    // for the bound here too.
    let anc_where = |params: &[GenericParam]| -> Vec<WherePredicate> {
        let names: HashSet<String> = params.iter().map(param_name).collect();
        union_where
            .iter()
            // `map_or`, not `is_some_and`: that one is stable since 1.70 and this crate's MSRV is 1.61.
            .filter(|p| where_pred_param(p).map_or(false, |id| names.contains(&id.to_string())))
            .cloned()
            .collect()
    };
    // Whether every `visit_*` carries `where Self: Sized`: this visitor's own heterogeneous mode, or
    // a base that is in one. Wider than `struct_only`, which is only about *this* visitor's closures.
    let sized_self = m.sized_self;
    let visit_tr = side.visit_trait();
    let into_vis_tr = side.ty("IntoVisitor");
    let into_hook_tr = side.ty("__VisitingIntoHook");
    let hook_tr = side.ty("__VisitingHook");
    let driver = side.ty("__VisitingDriver");
    let into_vis_fn = side.func("into_visitor");
    let into_hook_fn = side.func("into_hook");
    let visit_method = side.func("visit");
    let amp = side.amp();
    let recv = side.recv();
    let self_ret = if side.mutable {
        quote!(&mut Self)
    } else {
        quote!(&Self)
    };

    // Generated helper type params, named to avoid collision with the visited types' own generic
    // params (which arrive verbatim via `#g_params`). A visited type may thus declare a param
    // literally named `__V`/`__T`/`__H`/`__F`/`__A`/`__B` (or `__F0`…).
    let reserved: HashSet<String> = g_params.iter().map(param_name).collect();
    let p_v = fresh_ident("__V", &reserved);
    let p_t = fresh_ident("__T", &reserved);
    let p_h = fresh_ident("__H", &reserved);
    let p_f = fresh_ident("__F", &reserved);
    // Per-method generics for the container-edit views (`visit_*_seq`/`_opt` take `&mut impl SeqView<T>`
    // / `OptView<T>` — the field type itself, no wrapper).
    let p_a = fresh_ident("__A", &reserved);
    let p_b = fresh_ident("__B", &reserved);
    let p_vw = fresh_ident("__VW", &reserved);
    // The seq views' lifetime. A seq view's bound is `SeqView<T> + Elements<'lt, T>` and its slot
    // arrives as `&'lt V`, so an override can reach the container's own iterator; `Elements<'lt, T>`
    // declares `T: 'lt`, which the method repeats as a `where`-predicate. Minted against the visited
    // types' *method* params too, since those land in the same generic list.
    let lt_reserved: HashSet<String> = reserved
        .iter()
        .cloned()
        .chain(
            vtypes
                .iter()
                .flat_map(|t| t.method_params.iter().map(param_name)),
        )
        .collect();
    let view_lt = {
        let id = fresh_ident("__visiting_vw", &lt_reserved);
        syn::Lifetime::new(&format!("'{id}"), Span::call_site())
    };
    // The conversions name their result as an associated type rather than returning `impl Trait`
    // from a trait method: that spelling is only stable from 1.75, and every one of these impls
    // knows its concrete adapter anyway.
    let a_vis = fresh_ident("__Visitor", &reserved);
    let hooks_mod = side.func("__visiting_hooks");
    let a_hook = fresh_ident("__Hooked", &reserved);
    let p_ow = fresh_ident("__OW", &reserved);

    /// One container-edit view (`#[seq]` or `#[opt]`) generated for a visited type. Replaces the
    /// paired `has_seq`/`has_opt` bools + `seq_method`/`opt_method` + `seq_doc`/`opt_doc` fields on `S`
    /// with a single `Vec` so `trait_def`/`blanket_ref_impl` iterate once instead of two parallel
    /// `#(if ..)` blocks.
    struct ViewSpec {
        /// `visit_<name>_seq` / `visit_<name>_opt`.
        method: Ident,
        /// Doc string for the trait method (was `S::seq_doc`/`S::opt_doc`).
        doc: String,
        /// The per-method generic naming the view type: `p_vw` for seq, `p_ow` for opt (the *same*
        /// idents `gen_side` already mints once via `fresh_ident` — not reminted per type/kind).
        view_param: Ident,
        /// `'__visiting_vw,` for a seq view (its `Elements` bound names it); empty for an opt, which
        /// holds at most one element and so needs no iterator. Emitted *first* in the method's
        /// generic list, where a lifetime has to go.
        lt_decl: TokenStream,
        /// The whole bound on `view_param`: `SeqView<Ty> + Elements<'__visiting_vw, Ty>` for a seq,
        /// `OptView<Ty>` for an opt.
        bound: TokenStream,
        /// The view argument's reference: `&'__visiting_vw` / `&'__visiting_vw mut` for a seq (the lifetime
        /// its `Elements` bound is quantified over), plain `&` / `&mut` for an opt.
        amp: TokenStream,
        /// The method's `where`-clause: the type's own trait-method predicates, plus `Ty: '__visiting_vw`
        /// for a seq — which `Elements<'__visiting_vw, Ty>` requires and rustc will not infer.
        where_clause: TokenStream,
        /// Trait-method default body: the container's own iterator for a seq ([`Elements`]), an
        /// `if let Some(..) = get(v)` for an opt.
        default_body: TokenStream,
    }

    impl ViewSpec {
        /// The view a visited type gets for one container shape. Seq and opt differ in the view
        /// trait, the accessor and the wording; everything downstream of here treats them alike.
        #[allow(clippy::too_many_arguments)]
        fn new(
            kind: Container,
            side: Side,
            ident: &Ident,
            method: &Ident,
            view_param: Ident,
            ty: &TokenStream,
            lt: &syn::Lifetime,
            trait_where_preds: &[WherePredicate],
        ) -> Self {
            let (name, mname) = (ident.to_string(), method.to_string());
            let view_trait = kind.view_trait(side.mutable);
            let doc = match (kind, side.mutable) {
                (Container::Seq, true) => format!(
                    "Structurally edit the `{name}` nodes in a `Vec`-like parent slot via a \
                     [`SeqEdit`](__visiting_rt::SeqEdit) (`push`/`insert`/`remove`/`retain_mut`, \
                     `&mut v[i]`, and `v.elements_mut()`); default descends each via `{mname}`."
                ),
                (Container::Seq, false) => format!(
                    "Observe the `{name}` nodes in a `Vec`-like parent slot via a \
                     [`SeqView`](__visiting_rt::SeqView) (`len`, `&v[i]`, and `v.elements()`) — the \
                     slot itself, not just each element; default descends each via `{mname}`."
                ),
                (Container::Opt, true) => format!(
                    "Structurally edit the `{name}` node in an `Option`-like parent slot via an \
                     [`OptEdit`](__visiting_rt::OptEdit) (`get_mut`/`set`/`take`); default \
                     descends it via `{mname}`."
                ),
                (Container::Opt, false) => format!(
                    "Observe the `{name}` node in an `Option`-like parent slot via an \
                     [`OptView`](__visiting_rt::OptView) (`is_some`/`get`) — the slot itself, \
                     not just the element; default descends it via `{mname}`."
                ),
            };
            // Every view is quantified over `lt` now: all four have `Elements<'lt, _>` in their
            // supertrait chain, so the slot comes in as `&lt V` / `&lt mut V`, and `Ty: 'lt` has to
            // hold for the bound to be well-formed. One shape for both kinds.
            let lt_decl = quote!(#lt,);
            let bound = quote!( #view_trait<#lt, #ty> );
            // `&'lt` / `&'lt mut` — the lifetime goes *inside* the reference, so this cannot be
            // `side.amp()`, which is already the whole `&mut`.
            let amp = if side.mutable {
                quote!( & #lt mut )
            } else {
                quote!( & #lt )
            };
            let where_clause = {
                let mut preds = trait_where_preds.to_vec();
                preds.push(parse_quote!( #ty: #lt ));
                where_clause(&preds)
            };
            let default_body = match kind {
                // Through the container's own iterator, which is the whole point of the `Elements`
                // half of the bound: a `Vec` walks a slice, a `Punctuated` its own `Iter`, and a
                // shape whose `Index` is not O(1) is not re-indexed per element. `elements_mut`
                // takes the view's whole `lt` borrow, so it is a single pass — an override that
                // wants to iterate *and* resize indexes instead (`&mut v[i]`, then `push`).
                Container::Seq => quote! {
                    for __visiting_e in __visiting_rt::Elements::#{side.func("elements")}(v) {
                        self.#method(__visiting_e);
                    }
                },
                Container::Opt => quote! {
                    if let ::core::option::Option::Some(__visiting_e) = #view_trait::#{side.func("get")}(v) {
                        self.#method(__visiting_e);
                    }
                },
            };
            ViewSpec {
                method: side.func(&format!("visit_{}_{}", to_snake(ident), kind.word())),
                doc,
                view_param,
                lt_decl,
                bound,
                amp,
                where_clause,
                default_body,
            }
        }
    }

    struct S {
        ty: TokenStream,
        /// The same type, named through this module's `__visiting_ty_*` alias. The hook machinery sits in
        /// a submodule, where a `super::`-relative path the caller wrote would resolve one level too
        /// high; the alias is a plain name the submodule picks up by glob.
        ty_in_mod: TokenStream,
        /// The visited type's bare name (e.g. `Expr`), for generated doc comments.
        name: String,
        /// Generated doc for the trait method (`fn visit_<name>`) and the free fn (`visit_<name>`).
        tdoc: String,
        fdoc: String,
        method: Ident,
        /// Container-edit views (`visit_<name>_seq`/`_opt`); emitted only where the type is held that way.
        views: Vec<ViewSpec>,
        /// This type's non-shared params (heterogeneous mode), as the trait method's generics —
        /// lifetimes-first; empty in union mode.
        method_params: Vec<GenericParam>,
        /// The free fn's full generic list (trait params ∪ this type's non-shared params), normalized
        /// lifetimes-first so a non-shared lifetime never lands after a type/const param.
        free_params: Vec<GenericParam>,
        /// `where`-clause for the trait method (`where Self: Sized` in struct-only mode, plus any bound
        /// on this type's method-generic params, e.g. `S: Bound`); empty in the common union mode.
        trait_where: TokenStream,
        /// `where`-clause for the free fn — the union predicates (trait-level) plus this type's
        /// method-generic-param bounds; covers naming `Bounded<S>` where `S` is a method generic.
        free_where: TokenStream,
        hook: Ident,
        hook_struct: Ident,
        body: TokenStream,
    }
    let sides: Vec<S> = vtypes
        .iter()
        .map(|t| {
            let ident = t.ident.clone();
            let own = &t.own_use;
            let path = &t.path;
            let ty = quote!( #path #own );
            let ty_alias = Ident::new(&format!("__visiting_ty_{ident}"), Span::call_site());
            let ty_in_mod = quote!( #ty_alias #own );
            let mut method_params = t.method_params.clone();
            sort_lifetimes_first(&mut method_params);
            let mut free_params: Vec<GenericParam> =
                g_params.iter().cloned().chain(t.method_params.iter().cloned()).collect();
            sort_lifetimes_first(&mut free_params);
            // This type's `where`-bounds on its method-generic params (e.g. `S: Bound` when `S` is
            // non-shared). They can't live on the trait (it's keyed on the shared params), so they ride
            // the per-type `visit_*` method + free fn that declares the param.
            let mp_names: HashSet<String> = t.method_params.iter().map(param_name).collect();
            let method_where: Vec<WherePredicate> = t
                .own_where
                .iter()
                .filter(|p| where_pred_param(p).map_or(false, |id| mp_names.contains(&id.to_string())))
                .cloned()
                .collect();
            // Trait method: `where Self: Sized` (whenever the chain demands it) + the method-param
            // bounds. An extender of a heterogeneous base needs it too: its own default body calls a
            // free fn that is now sized, and the supertrait's methods are sized regardless.
            let trait_where_preds: Vec<WherePredicate> = if sized_self {
                let mut preds: Vec<WherePredicate> = vec![parse_quote!(Self: ::core::marker::Sized)];
                preds.extend(method_where.iter().cloned());
                preds
            } else {
                Vec::new()
            };
            let trait_where = where_clause(&trait_where_preds);
            // Free fn: the trait-level union predicates + this type's method-param bounds.
            let free_where = {
                let mut preds = union_where.to_vec();
                preds.extend(method_where.iter().cloned());
                where_clause(&preds)
            };
            let method = side.method(&ident);
            let name = ident.to_string();
            let mname = method.to_string();
            let tdoc = format!(
                "Visit an `{name}` node; the default recurses via [`{mname}`]. Override to act, calling \
                 `{mname}(self, i)` to keep descending."
            );
            let fdoc = format!(
                "Recurse into an `{name}`'s children, dispatching each to `visit_*{mut_sfx}` \
                 ([`{visit_tr}::{mname}`]'s default delegates here).",
                mut_sfx = side.fn_suffix(),
            );
            let mut views = Vec::new();
            // Both sides get the views. The shared one observes the parent slot — its length, its
            // neighbours, an element's index — which `visit_<t>` alone cannot show; the `&mut` one
            // edits it. Named `_seq`/`_opt` on `Visit` and `_seq_mut`/`_opt_mut` on `VisitMut`.
            for (kind, used, view_param) in [
                (Container::Seq, seq_used, &p_vw),
                (Container::Opt, opt_used, &p_ow),
            ] {
                if used.contains(&name) {
                    views.push(ViewSpec::new(
                        kind,
                        side,
                        &ident,
                        &method,
                        view_param.clone(),
                        &ty,
                        &view_lt,
                        &trait_where_preds,
                    ));
                }
            }
            S {
                ty,
                ty_in_mod,
                name,
                tdoc,
                fdoc,
                method,
                views,
                method_params,
                free_params,
                trait_where,
                free_where,
                hook: side.func(&format!("hook_{}", to_snake(&ident))),
                hook_struct: side.ty(&format!("__Visiting{ident}Hook")),
                body: if side.mutable { t.body_mut.clone() } else { t.body.clone() },
            }
        })
        .collect();

    let tup = tuple_impls(8, g_params, g_args, side, union_where);
    // The union of every visited type's `where`-predicates, repeated on each generated item that is
    // quantified over the full param union (the trait, free fns, the `&mut V` / Driver / closure /
    // Chain impls) so a visited type like `enum Expr<S> where S: Bound { .. }` stays well-formed.
    let uw = where_clause(union_where);

    let visited_list = sides
        .iter()
        .map(|s| format!("`{}`", s.name))
        .collect::<Vec<_>>()
        .join(", ");
    let entry = visit_method.to_string();
    let trait_doc = format!(
        "Visitor over {visited_list} (generated by `define_visit!`). Override the `visit_*{mut_sfx}` methods \
         you care about — each default recurses into that node's children; start with \
         `node.{entry}(&mut visitor)`.{base_note}",
        mut_sfx = side.fn_suffix(),
        base_note = if side.mutable { " The by-`&mut` variant of `Visit`." } else { "" },
    );
    // In method-mode the visited set is heterogeneous, so the trait's methods carry their own
    // generics and a closure cannot implement it (a closure is not `for<T>` generic). Say so here:
    // passing one otherwise fails as a bare `expected &mut _, found closure` type mismatch, which
    // names neither the closure machinery nor the reason it is absent.
    let inherent_doc = if struct_only {
        format!(
            "Visit `self` with any `{visit_tr}`, returning `self` to chain. This visitor is \
             heterogeneous — some visited type fills another's parameter concretely, or bounds it — \
             so `{visit_tr}`'s methods are themselves generic and a **closure cannot be used** \
             here; pass `&mut` a type implementing `{visit_tr}`."
        )
    } else {
        format!("Visit `self` with any `{visit_tr}`, returning `self` to chain.")
    };

    // Inherent `visit` / `visit_mut` per type (replaces the Visitable trait). Each type's own
    // params go on the impl; any extra union params go on the method (so a type that doesn't use
    // every union param doesn't leave the impl param unconstrained). The type's own `where`-clause
    // (referencing only its own params) goes on the impl so naming `Expr<S>` stays well-formed.
    let inherent: Vec<TokenStream> = vtypes
        .iter()
        .map(|vt| {
            // A foreign target can't carry an inherent impl (E0116); callers use `Visit::visit_*`.
            if !vt.local {
                return quote!();
            }
            let own_names: HashSet<String> = vt.own_params.iter().map(param_name).collect();
            let extra: Vec<&GenericParam> = g_params
                .iter()
                .filter(|p| !own_names.contains(&param_name(p)))
                .collect();
            let own_def = angle(&vt.own_params);
            let own_w = where_clause(&vt.own_where);
            let path = &vt.path;
            let own_use = &vt.own_use;
            let method = side.method(&vt.ident);
            if struct_only {
                // Direct `&mut impl Visit` (the closure/`IntoVisitor` machinery is off in method-mode).
                quote! {
                    impl #own_def #path #own_use #own_w {
                        #[doc = #inherent_doc]
                        pub fn #visit_method< #(#extra,)* #p_v: #visit_tr #g_use >(
                            #recv,
                            visitor: &mut #p_v,
                        ) -> #self_ret {
                            visitor.#method(self);
                            self
                        }
                    }
                }
            } else {
                quote! {
                    impl #own_def #path #own_use #own_w {
                        #[doc = #inherent_doc]
                        pub fn #visit_method< #(#extra,)* #p_t >(
                            #recv,
                            visitor: impl #into_vis_tr< #(#g_args,)* #p_t >,
                        ) -> #self_ret {
                            let mut visitor = visitor.#into_vis_fn();
                            visitor.#method(self);
                            self
                        }
                    }
                }
            }
        })
        .collect();

    let trait_def = quote! {
        #[doc = #trait_doc]
        pub trait #visit_tr #g_def #(if let Some(b) = base) { : #b::#visit_tr #base_g_use } #uw {
            #(for s in &sides) {
                // In heterogeneous (struct-only) mode the method carries this type's non-shared params
                // as generics, and `where Self: Sized` (the method-generic dispatch needs a sized Self).
                #[doc = #{&s.tdoc}]
                fn #{&s.method}< #(for mp in &s.method_params) { #mp, } >(&mut self, i: #amp #{&s.ty})
                    #{&s.trait_where}
                {
                    #{&s.method}(self, i)
                }
                #(for spec in &s.views) {
                    #[doc = #{&spec.doc}]
                    fn #{&spec.method}< #{&spec.lt_decl} #(for mp in &s.method_params) { #mp, } #{&spec.view_param}: #{&spec.bound} >(
                        &mut self,
                        v: #{&spec.amp} #{&spec.view_param},
                    ) #{&spec.where_clause} {
                        #{&spec.default_body}
                    }
                }
            }
        }
    };

    // A visitor can be reached through a wrapper, or several can ride along together; either way every
    // method forwards to the same place. One shape, three headers: `&mut V` and `Box<V>` forward to
    // their single delegate, a tuple to each element in turn, and the container-edit views ride along
    // with the per-node methods.
    // Emitted in heterogeneous mode too. Only the *closure* machinery is impossible there (a closure
    // cannot be `for<T>` generic); forwarding is not — the per-method generics and the method's own
    // `where` ride along, and every wrapper is `Sized`, which is all `where Self: Sized` asks. Without
    // these a visitor in that mode cannot be passed by `&mut` or boxed, and — because the forwarding
    // impls of an extending visitor need the base's — cannot be extended at all.
    let forwarding_impl = |header: TokenStream, delegates: Vec<(TokenStream, TokenStream)>| {
        quote! {
            #header #uw {
                #(for s in &sides) {
                    fn #{&s.method}< #(for mp in &s.method_params) { #mp, } >(&mut self, i: #amp #{&s.ty})
                        #{&s.trait_where}
                    {
                        #(for (v, recv) in &delegates) {
                            <#v as #visit_tr #g_use>::#{&s.method}(#recv, i);
                        }
                    }
                    #(for spec in &s.views) {
                        fn #{&spec.method}< #{&spec.lt_decl} #(for mp in &s.method_params) { #mp, } #{&spec.view_param}: #{&spec.bound} >(&mut self, v: #{&spec.amp} #{&spec.view_param}) #{&spec.where_clause} {
                            #(for (d, recv) in &delegates) {
                                <#d as #visit_tr #g_use>::#{&spec.method}(#recv, v);
                            }
                        }
                    }
                }
            }
        }
    };

    // The receiver a caller passes to `node.visit(&mut pass)`.
    let blanket_ref_impl = forwarding_impl(
        quote!( impl< #(#g_params,)* #p_v: #visit_tr #g_use > #visit_tr #g_use for &mut #p_v ),
        vec![(quote!(#p_v), quote!(self))],
    );

    // A boxed visitor is a visitor, so `node.visit(Box::new(pass))` works. Written for `Box`
    // specifically rather than blanketed over a wrapper trait, which a consumer could then implement
    // for a type this crate also covers.
    let boxed_visitor_impl = forwarding_impl(
        // `?Sized` only where the methods allow it: in heterogeneous mode each carries
        // `where Self: Sized`, so forwarding to the inner visitor needs it sized.
        quote!( impl< #(#g_params,)* #p_v: #visit_tr #g_use #(if !sized_self) { + ?Sized } > #visit_tr #g_use for ::std::boxed::Box<#p_v> ),
        vec![(quote!(#p_v), quote!(&mut **self))],
    );

    // Each visited node implements `Walk`/`WalkMut` by handing itself to the visitor. Unconditional
    // (no `FieldTy: Walk` predicates): those belong on the free fns, and putting them here makes a
    // mutually recursive AST overflow the trait solver instead of terminating.
    let walk_tr = side.walk_trait();
    let walk_fn = side.walk_fn();
    let walk_recv = side.recv();
    let tag = walk_tag();
    let tag_args = walk_tag_use(g_params);
    let walk_impls: Vec<TokenStream> = sides
        .iter()
        .map(|s| {
            quote! {
                impl< #(for gp in &s.free_params) { #gp, } #p_v: #visit_tr #g_use #(if !sized_self) { + ?Sized } >
                    __visiting_rt::walk::#walk_tr< #tag #tag_args, __visiting_rt::walk::Here, #p_v >
                    for #{&s.ty} #{&s.free_where}
                {
                    fn #walk_fn(#walk_recv, v: &mut #p_v) {
                        <#p_v as #visit_tr #g_use>::#{&s.method}(v, self)
                    }
                }
            }
        })
        .collect();

    let free_fns = quote! {
        #(for s in &sides) {
            // No `?Sized` under struct-only: the body may dispatch through `Self`'s method-generic
            // `visit_*` (which requires `Self: Sized`). `free_params` = trait params ∪ this type's
            // non-shared params, lifetimes-first.
            #[doc = #{&s.fdoc}]
            pub fn #{&s.method}< #(for gp in &s.free_params) { #gp, } #p_v: #visit_tr #g_use #(if !sized_self) { + ?Sized } >(
                this: &mut #p_v,
                i: #amp #{&s.ty},
            ) #{&s.free_where} {
                #{&s.body}
            }
            // The `visit_*_seq`/`_opt` container-edit views have no free fn: their default descent is
            // inlined into the trait-method default (they just iterate the view calling `visit_*_mut`).
        }
    };

    // One hook trait per ancestor, because the `Driver` impl of an ancestor's `Visit` can only be
    // quantified over *that* ancestor's parameters: a bound naming the full union there would leave
    // the extra parameters unconstrained (E0207). Each carries the methods for the types that
    // ancestor declares, so a closure over an inherited type has somewhere to land.
    struct AncHook {
        tr: Ident,
        /// The ancestor, reached through the `__visiting_base` relay rather than by the path the caller
        /// wrote: a `super::`-relative one would mean something else inside the hooks module.
        path_in_mod: TokenStream,
        g_params: Vec<GenericParam>,
        g_use: TokenStream,
        /// The union predicates about this ancestor's own params, repeated on every item that names
        /// its trait or its types. See `anc_where`.
        uw: TokenStream,
        /// `(hook fn, hook struct, visit method, type, type as the hooks module names it)`
        methods: Vec<(Ident, Ident, Ident, TokenStream, TokenStream)>,
    }
    let relay_seg = Ident::new(BASE_RELAY, Span::call_site());
    let anc_hooks: Vec<AncHook> = ancestors
        .iter()
        .enumerate()
        .map(|(i, a)| AncHook {
            tr: side.ty(&format!("AncHook{i}")),
            path_in_mod: {
                let hops: Vec<&Ident> = vec![&relay_seg; i + 1];
                let (first, rest) = hops.split_first().expect("at least one hop");
                quote!( #first #(:: #rest)* )
            },
            g_params: a.g_params.clone(),
            g_use: a.g_use.clone(),
            uw: where_clause(&anc_where(&a.g_params)),
            methods: a
                .declares
                .iter()
                // A method-generic one has no hook: a closure cannot be `for<T>` generic, and the
                // ancestor's `Visit` default walks it anyway.
                .filter(|d| !d.method_generic)
                .map(|d| {
                    let path = m
                        .reachable
                        .get(&d.ident.to_string())
                        .cloned()
                        .unwrap_or_else(|| {
                            syn::parse2(quote!(#{&d.ident})).expect("ident is a path")
                        });
                    let args = &d.args;
                    let alias =
                        Ident::new(&format!("__visiting_ty_{}", d.ident), Span::call_site());
                    let hops = vec![&relay_seg; i + 1];
                    (
                        side.func(&format!("hook_{}", to_snake(&d.ident))),
                        side.ty(&format!("__Visiting{}InhHook", d.ident)),
                        side.method(&d.ident),
                        quote!( #path #args ),
                        quote!( #(#hops ::)* #alias #args ),
                    )
                })
                .collect(),
        })
        .collect();
    let closure_machinery = quote! {
        #(if !struct_only) {
        pub trait #into_vis_tr< #(#g_params,)* #p_t > #uw {
            type #a_vis: #visit_tr #g_use;
            fn #into_vis_fn(self) -> Self::#a_vis;
        }

        // Closures: shallow Hook + single-pass Driver. These are the closure adapters' own
        // machinery — a user names `IntoVisitor` (via `node.visit(..)`), never these. They have to
        // be `pub`, since `IntoVisitor` reports its result as an associated type and a public trait
        // cannot name a private one. Keeping them in a module of their own is what stops a
        // `use path::to::visit::*;` from pulling them in: a glob takes the module's *name*, not its
        // contents, so one hidden name is imported instead of a dozen.
        #[doc(hidden)]
        pub mod #hooks_mod {
            use super::*;

            impl< #(#g_params,)* #p_v: #visit_tr #g_use > #into_vis_tr< #(#g_args,)* () > for #p_v #uw {
                type #a_vis = Self;
                fn #into_vis_fn(self) -> Self::#a_vis { self }
            }
        // The traits chain the way the visitors do: an ancestor's `Visit` has the next one up as a
        // supertrait, so proving `Driver<H>` visits at one level needs `H` to answer every level
        // above it too. Each ancestor's parameters are a subset of the ones below, so the bound is
        // always nameable.
        #(for (i, h) in anc_hooks.iter().enumerate()) {
            #[doc(hidden)]
            pub trait #{&h.tr}< #(for p in &h.g_params) { #p, } >
                #(if let Some(up) = anc_hooks.get(i + 1)) { : #{&up.tr} #{&up.g_use} }
                #{&h.uw}
            {
                #(for (hook, _, _, _, ty) in &h.methods) {
                    fn #hook(&mut self, i: #amp #ty) { let _ = i; }
                }
            }
        }
        #[doc(hidden)]
        pub trait #hook_tr #g_def #(if let Some(h0) = anc_hooks.first()) { : #{&h0.tr} #{&h0.g_use} } #uw {
            #(for s in &sides) {
                fn #{&s.hook}(&mut self, i: #amp #{&s.ty_in_mod}) { let _ = i; }
            }
        }
        #[doc(hidden)]
        pub trait #into_hook_tr< #(#g_params,)* #p_t > #uw {
            type #a_hook: #hook_tr #g_use;
            fn #into_hook_fn(self) -> Self::#a_hook;
        }

        #[doc(hidden)]
        pub struct #driver<#p_h>(pub #p_h);
        impl< #(#g_params,)* #p_h: #hook_tr #g_use > #visit_tr #g_use for #driver<#p_h> #uw {
            #(for s in &sides) {
                fn #{&s.method}(&mut self, i: #amp #{&s.ty_in_mod}) {
                    self.0.#{&s.hook}(i);
                    #{&s.method}(self, i);
                }
            }
        }
        // The new trait extends the base (transitively), so Driver must satisfy *every* ancestor
        // supertrait (via their defaults). Each empty impl is quantified over only that ancestor's
        // params (+ the wrapped hook) so a wider new-union param is not an unconstrained impl param.
        #(for h in &anc_hooks) {
            impl< #(for p in &h.g_params) { #p, } #p_h: #{&h.tr} #{&h.g_use} >
                #{&h.path_in_mod}::#visit_tr #{&h.g_use} for #driver<#p_h>
                #{&h.uw}
            {
                #(for (hook, _, method, _, ty) in &h.methods) {
                    fn #method(&mut self, i: #amp #ty) {
                        self.0.#hook(i);
                        #{&h.path_in_mod}::#method(self, i);
                    }
                }
            }
        }

        // A hook carries one closure, so it answers one trait in the family and takes the defaults
        // for the rest. Every one of them has to be answered all the same: `Driver<H>` is a visitor
        // only once `H` satisfies this module's `Hook` *and* each ancestor's.
        #(for s in &sides) {
            #(for h in &anc_hooks) {
                impl< #(for p in &h.g_params) { #p, } #p_f >
                    #{&h.tr} #{&h.g_use} for #{&s.hook_struct}<#p_f> #{&h.uw} {}
            }
        }
        #(for (hi, h) in anc_hooks.iter().enumerate()) {
            #(for (hook, hook_struct, _, _, ty) in &h.methods) {
                #[doc(hidden)]
                pub struct #hook_struct<#p_f>(pub #p_f);
                impl< #(for p in &h.g_params) { #p, } #p_f: ::core::ops::FnMut( #amp #ty ) >
                    #{&h.tr} #{&h.g_use} for #hook_struct<#p_f>
                    #{&h.uw}
                {
                    fn #hook(&mut self, i: #amp #ty) { (self.0)(i); }
                }
                // The bound is repeated here: `Hook` has the ancestor traits as supertraits, so
                // this impl has to let the one that carries the closure be proven.
                impl< #(#g_params,)* #p_f: ::core::ops::FnMut( #amp #ty ) >
                    #hook_tr #g_use for #hook_struct<#p_f> #uw {}
                // A trait below this one reaches it through the supertrait chain, so its impl
                // has to carry the bound as well; its parameters are a superset, so naming the
                // closure's argument there is fine. One above never reaches back down, and its
                // parameters may not even cover the argument, so it stays bare.
                #(for (oi, other) in anc_hooks.iter().enumerate()) {
                    #(if oi < hi) {
                        impl< #(for p in &other.g_params) { #p, } #p_f: ::core::ops::FnMut( #amp #ty ) >
                            #{&other.tr} #{&other.g_use} for #hook_struct<#p_f> #{&other.uw} {}
                    }
                    #(if oi > hi) {
                        impl< #(for p in &other.g_params) { #p, } #p_f >
                            #{&other.tr} #{&other.g_use} for #hook_struct<#p_f> #{&other.uw} {}
                    }
                }
                impl< #(#g_params,)* #p_f: ::core::ops::FnMut( #amp #ty ) >
                    #into_hook_tr< #(#g_args,)* #ty > for #p_f #uw
                {
                    type #a_hook = #hook_struct<#p_f>;
                    fn #into_hook_fn(self) -> Self::#a_hook { #hook_struct(self) }
                }
                impl< #(#g_params,)* #p_f: ::core::ops::FnMut( #amp #ty ) >
                    #into_vis_tr< #(#g_args,)* #ty > for #p_f #uw
                {
                    type #a_vis = #driver<#hook_struct<#p_f>>;
                    fn #into_vis_fn(self) -> Self::#a_vis { #driver(#hook_struct(self)) }
                }
            }
        }

        #(for s in &sides) {
            #[doc(hidden)]
            pub struct #{&s.hook_struct}<#p_f>(pub #p_f);
            impl< #(#g_params,)* #p_f: ::core::ops::FnMut( #amp #{&s.ty_in_mod} ) >
                #hook_tr #g_use for #{&s.hook_struct}<#p_f> #uw
            {
                fn #{&s.hook}(&mut self, i: #amp #{&s.ty_in_mod}) { (self.0)(i); }
            }
            impl< #(#g_params,)* #p_f: ::core::ops::FnMut( #amp #{&s.ty_in_mod} ) >
                #into_hook_tr< #(#g_args,)* #{&s.ty_in_mod} > for #p_f #uw
            {
                type #a_hook = #{&s.hook_struct}<#p_f>;
                fn #into_hook_fn(self) -> Self::#a_hook { #{&s.hook_struct}(self) }
            }
            impl< #(#g_params,)* #p_f: ::core::ops::FnMut( #amp #{&s.ty_in_mod} ) >
                #into_vis_tr< #(#g_args,)* #{&s.ty_in_mod} > for #p_f #uw
            {
                type #a_vis = #driver<#{&s.hook_struct}<#p_f>>;
                fn #into_vis_fn(self) -> Self::#a_vis { #driver(#{&s.hook_struct}(self)) }
            }
        }

        // Multiple closures: a 2-tuple of hooks is itself a hook (calls both), so it is the
        // tuple-of-closures combinator directly — no `Chain` newtype. `build_chain` nests them right.
        impl< #(#g_params,)* #p_a: #hook_tr #g_use, #p_b: #hook_tr #g_use >
            #hook_tr #g_use for ( #p_a, #p_b ) #uw
        {
            #(for s in &sides) {
                fn #{&s.hook}(&mut self, i: #amp #{&s.ty_in_mod}) {
                    self.0.#{&s.hook}(i);
                    self.1.#{&s.hook}(i);
                }
            }
        }
        #(for h in &anc_hooks) {
            impl< #(for p in &h.g_params) { #p, } #p_a: #{&h.tr} #{&h.g_use}, #p_b: #{&h.tr} #{&h.g_use} >
                #{&h.tr} #{&h.g_use} for ( #p_a, #p_b )
                #{&h.uw}
            {
                #(for (hook, _, _, _, ty) in &h.methods) {
                    fn #hook(&mut self, i: #amp #ty) {
                        self.0.#hook(i);
                        self.1.#hook(i);
                    }
                }
            }
        }
        #(for imp in &tup) { #imp }
        } // end mod #hooks_mod
        } // end #(if !struct_only) — closure/Driver machinery off for a recurse base
    };

    quote! {
        #trait_def
        #(for imp in &walk_impls) { #imp }
        #blanket_ref_impl
        #boxed_visitor_impl
        #free_fns
        #closure_machinery
        // Inherent entry points (no trait import needed at the call site).
        #(for imp in &inherent) { #imp }
    }
}
