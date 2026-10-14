use super::*;

/// A fetched AST type: the path it was fetched by, its (cleaned) definition, and its `#[visiting]`.
pub(crate) struct DoneType {
    pub(crate) path: Path,
    pub(crate) def: Item,
    pub(crate) subast: Vec<SubEntry>,
}

// `__visitor_build`: receives accumulated state + the just-resolved definition, fetches the next type
// or generates the module.

/// One visitor being extended, as its reply left it: the path this module names it by, the generic
/// params its trait carries, the targets it declares, whether its methods demand `Self: Sized`, and
/// its identity. Kept per base rather than merged — `base::Visit<..>` has to be spelled with *that*
/// base's arity, and its free fns and type aliases relayed under its own hop.
pub(crate) struct BaseIn {
    pub(crate) path: Path,
    pub(crate) generics: Vec<GenericParam>,
    pub(crate) own: Vec<Decl>,
    pub(crate) sized_self: bool,
    pub(crate) id: TokenStream,
    /// The identities of the visitors this base itself extends. See [`AncIn::up`].
    pub(crate) up: Vec<TokenStream>,
}

pub(crate) struct BuildInput {
    /// Every visitor this one extends, in the order written. Each becomes a supertrait.
    pub(crate) bases: Vec<BaseIn>,
    /// Bases whose metadata has not been asked for yet. The one being asked *now* travels as
    /// `@bnow` and is consumed while parsing — a reply carries no name of its own, so it has to be
    /// told which base it answers for — and so is not kept here.
    pub(crate) base_fetch: Vec<Path>,
    pub(crate) build: Path,
    pub(crate) nonce: TokenStream,
    pub(crate) visited: Vec<Path>,
    /// Types inherited from the base, as `path as matchkey` rather than bare idents, so a field
    /// holding an inherited type is followed without a `#[visiting]` entry repeating what the base
    /// already knows. Requalified against the base path on arrival, so the paths stay resolvable
    /// through any number of generations.
    pub(crate) inherited: Vec<SubEntry>,
    /// Everything the bases reach above themselves (for multi-level `base => mid => new`), so the
    /// new visitor can emit the empty `Driver` impl for every transitive supertrait, not just the
    /// direct ones. Deduplicated by identity, so a diamond yields one entry.
    pub(crate) base_ancestors: Vec<AncIn>,
    /// Path of the type whose `@ast`/`@subast` trail in this bounce (so the fetched def is recorded
    /// under the path it was fetched by). Empty before any type is fetched.
    pub(crate) fetching: Option<Path>,
    pub(crate) done: Vec<DoneType>,
    pub(crate) rest: Vec<Path>,
    pub(crate) just_def: Option<Item>,
    pub(crate) just_subast: Vec<SubEntry>,
}

impl BuildInput {
    /// Visited types' last-idents ∪ inherited idents — the set of heads that dispatch via a
    /// `visit_*` method call rather than being drilled/leaf.
    pub(crate) fn method_set(&self) -> HashSet<String> {
        self.visited
            .iter()
            .map(|p| last_ident(p).to_string())
            .chain(self.inherited.iter().map(|e| e.key.to_string()))
            .collect()
    }
}

/// Parse one `@<name> { .. }` section, returning the name and the braced content as tokens.
pub(crate) fn parse_section(input: ParseStream) -> Result<(Ident, TokenStream)> {
    input.parse::<Token![@]>()?;
    let name: Ident = input.parse()?;
    let content;
    braced!(content in input);
    Ok((name, content.parse()?))
}

/// One transitive-base obligation for multi-level inheritance: an ancestor visitor's path and the
/// names of its generic params (re-mapped into the extending visitor's union when emitted).
/// One type a visitor declares a `visit_*` method for, with the arguments that method's parameter
/// is written with (`Expr` + `<S>`). The arguments are carried rather than recomputed because an
/// extender knows an ancestor's parameters as a whole but not how each of its types divides them
/// up, and an override's signature has to match the trait's exactly.
///
/// A `?` after the name marks a *method-generic* declaration: in heterogeneous mode a type's
/// non-shared params ride its `visit_*` as generics, so the method is quantified per call. Such a
/// one carries no arguments — they name params the extender does not have — and nothing an extender
/// writes can be `for<T>` generic, so it gets neither a closure hook nor an `impl_chain!` arm. The
/// mark is kept rather than the entry dropped so the chain check can say which case it is.
#[derive(Clone)]
pub(crate) struct Decl {
    pub(crate) ident: Ident,
    pub(crate) args: TokenStream,
    pub(crate) method_generic: bool,
}

impl Parse for Decl {
    fn parse(input: ParseStream) -> Result<Self> {
        let ident: Ident = input.parse()?;
        let args = if input.peek(Token![<]) {
            let a: AngleBracketedGenericArguments = input.parse()?;
            quote!(#a)
        } else {
            TokenStream::new()
        };
        let method_generic = input.parse::<Option<Token![?]>>()?.is_some();
        Ok(Decl {
            ident,
            args,
            method_generic,
        })
    }
}

fn parse_decls(ts: TokenStream) -> Result<Vec<Decl>> {
    let parser = |input: ParseStream| {
        let mut out = Vec::new();
        while !input.is_empty() {
            out.push(input.parse::<Decl>()?);
        }
        Ok(out)
    };
    parser.parse2(ts)
}

pub(crate) fn emit_decls(decls: &[Decl]) -> TokenStream {
    let parts: Vec<TokenStream> = decls
        .iter()
        .map(|d| {
            let (i, a) = (&d.ident, &d.args);
            quote!( #i #a #(if d.method_generic) { ? } )
        })
        .collect();
    quote!( #(#parts)* )
}

pub(crate) struct AncIn {
    /// The ancestor as *this* module can name it: the base it came through, then its relay hops.
    pub(crate) path: Path,
    /// The hops themselves — `__visiting_base_<k>` segments, relative to the module that exports
    /// them. A downstream extender prefixes its own hop to reach the same ancestor, and the chain
    /// macros use them directly, since their bodies expand in the caller's crate.
    pub(crate) relay: Vec<Ident>,
    pub(crate) names: Vec<Ident>,
    /// The types whose `visit_*` methods this ancestor *declares* — its own `define_visit!` targets, not
    /// what it in turn inherited. An override for one of them belongs in the impl of this
    /// ancestor's trait and nowhere else, so the closure driver needs to know where each inherited
    /// type's method lives.
    pub(crate) declares: Vec<Decl>,
    /// The ancestor's own nonce. Two bases reach a shared ancestor by relay paths of their own, so
    /// the paths cannot say they mean the same module; this can, and a diamond needs it to write one
    /// impl rather than two conflicting ones.
    pub(crate) id: TokenStream,
    /// The identities of the visitors this ancestor itself extends. The chain is a graph once a
    /// visitor may have several bases — a shared ancestor sits above two of them at once — and the
    /// relay hops record only one way to reach it, so who-extends-whom is carried outright.
    pub(crate) up: Vec<TokenStream>,
}

/// Parse `@anc { @a { @p {PATH} @r {hop…} @n {name…} @d {decl…} @i {id} @u {{id}…} } … }`.
fn parse_ancestors(ts: TokenStream) -> Result<Vec<AncIn>> {
    let parser = |input: ParseStream| {
        let mut out = Vec::new();
        while !input.is_empty() {
            input.parse::<Token![@]>()?;
            let kw: Ident = input.parse()?;
            if kw != "a" {
                return Err(Error::new(kw.span(), "expected `@a` in @anc"));
            }
            let content;
            braced!(content in input);
            let mut path = None;
            let mut relay = Vec::new();
            let mut names = Vec::new();
            let mut declares = Vec::new();
            let mut id = TokenStream::new();
            let mut up = Vec::new();
            while !content.is_empty() {
                let (name, inner) = parse_section(&content)?;
                match name.to_string().as_str() {
                    "p" => path = Some(syn::parse2(inner)?),
                    "r" => relay = parse_idents(inner)?,
                    "n" => names = parse_idents(inner)?,
                    "d" => declares = parse_decls(inner)?,
                    "i" => id = inner,
                    "u" => up = parse_ids(inner)?,
                    other => {
                        return Err(Error::new(
                            name.span(),
                            format!("unknown @a section @{other}"),
                        ))
                    }
                }
            }
            let path: Path =
                path.ok_or_else(|| Error::new(Span::call_site(), "missing @p in @a"))?;
            out.push(AncIn {
                path,
                relay,
                names,
                declares,
                id,
                up,
            });
        }
        Ok(out)
    };
    parser.parse2(ts)
}

pub(crate) fn emit_ancestors(anc: &[AncIn]) -> TokenStream {
    let blocks: Vec<TokenStream> = anc
        .iter()
        .map(|a| {
            let (path, relay, names, id) = (&a.path, &a.relay, &a.names, &a.id);
            let declares = emit_decls(&a.declares);
            let up = emit_ids(&a.up);
            quote! {
                @a {
                    @p { #path } @r { #(#relay)* } @n { #(#names)* } @d { #declares }
                    @i { #id } @u { #up }
                }
            }
        })
        .collect();
    quote!( #(#blocks)* )
}

/// The `__visiting_visited` export — a `#[macro_export]` muncher that, when a downstream `define_visit!(self =>
/// New)` invokes it, appends everything this visitor can reach as `path as matchkey` (`@inh`), its
/// param union (`@bg`), and its ancestor chain (`@an`).
///
/// Paths, not bare idents: the extender needs to *name* an inherited type to emit its `Walk` impl and
/// to follow a field holding it, and it has no other way to learn where that type lives. The same
/// channel already carries the ancestor chain, and for the same reason.
pub(crate) fn emit_visited_macro(
    st: &BuildInput,
    g_params: &[GenericParam],
    anc_export: TokenStream,
    own: &[Decl],
    sized_self: bool,
) -> TokenStream {
    let all_visible: Vec<TokenStream> = st
        .visited
        .iter()
        .map(|p| {
            let k = last_ident(p);
            quote!( #p as #k )
        })
        .chain(st.inherited.iter().map(|e| {
            let (p, k) = (&e.path, &e.key);
            quote!( #p as #k )
        }))
        .collect();
    let own = emit_decls(own);
    let vmacro = Ident::new(
        &format!("__visiting_visited_{}", st.nonce),
        Span::call_site(),
    );
    quote! {
        // The embedded visited-type / ancestor paths may be `crate::`-rooted by design (they resolve in
        // the base's defining crate); suppress clippy's `crate_in_macro_def` for the generated macro.
        #[allow(clippy::crate_in_macro_def)]
        #[macro_export]
        #[doc(hidden)]
        macro_rules! #vmacro {
            (@visited $cb:path { $($pre:tt)* }) => {
                $cb ! {
                    $($pre)* @inh { #(#all_visible),* } @bg { #(#g_params),* } @an { #anc_export }
                    @own { #own } @sz { #(if sized_self) { yes } } @id { #{&st.nonce} }
                    @up { #{emit_ids(&st.bases.iter().map(|b| b.id.clone()).collect::<Vec<_>>())} }
                }
            };
        }
        #[doc(hidden)]
        pub use #vmacro as __visiting_visited;
    }
}

/// `impl_chain!`, the hidden helper that writes the ancestor impls a visitor would otherwise have
/// to spell out by hand.
///
/// The generated `Visit` has every ancestor's `Visit` as a supertrait, so a visitor type must
/// implement all of them even when it only overrides methods of this visitor's own types. Each of
/// those impls is usually empty — every method has a walking default — but Rust has no way to ask
/// for them, and their generic arity differs per ancestor. This writes them:
///
/// ```ignore
/// top::impl_chain!(top; MyPass);
/// ```
///
/// To act on an inherited node, hand the method to the macro rather than writing an impl of your
/// own, which would be a second impl of the same trait (E0119). The argument's type may be left
/// off — the macro knows it, and that is the one thing a caller would otherwise have to look up:
///
/// ```ignore
/// top::impl_chain! { top; MyPass;
///     fn visit_type(&mut self, i) { self.types += 1; base::visit_type(self, i); }
/// }
/// ```
///
/// Spelling the type out works too, for a signature you want to read at a glance.
///
/// `fn <name>;` with no body hands that ancestor back: nothing is generated for it, and you write
/// its impls yourself. That is the way out when a body wants ordinary tooling — rustfmt does not
/// reach inside a macro call:
///
/// ```ignore
/// top::impl_chain! { top; MyPass; fn visit_type; }
/// impl<S> base::Visit<S> for MyPass { fn visit_type(&mut self, i: &Type<S>) { .. } }
/// impl<S> base::VisitMut<S> for MyPass {}
/// ```
///
/// Everything is keyed by **method name**, which is unique across a chain — two ancestor *modules*
/// may share a last segment, so naming the ancestor would not be. One muncher per ancestor keeps
/// the methods that are its own and drops the rest, accumulating the two sides and the hand-back
/// markers separately; a last one rejects a name no ancestor declares, which would otherwise be
/// dropped in silence.
///
/// The module path is repeated as an argument because the body cannot name the ancestors any other
/// way: `$crate` here is *this* crate, not the expanding one — `define_visit!` reaches the proc macro
/// through a `macro_rules` wrapper in `visit`, and `$crate` resolves against that definition.
/// Taking the path instead makes the expansion position-independent: every ancestor is reached as
/// `<module>::__visiting_base…`, the relay chain, which needs no crate name and no re-rooting.
///
/// Emitted even with no ancestors (where it expands to nothing) so that code generating a visitor
/// can call it unconditionally.
pub(crate) fn emit_impl_chain(nonce: &TokenStream, ancestors: &[Ancestor]) -> TokenStream {
    let name = Ident::new(&format!("__visiting_impl_chain_{nonce}"), Span::call_site());
    // The re-exports live one module down, so `use path::to::visit::*;` brings in none of them.
    let chain_mod = Ident::new(CHAIN_MOD, Span::call_site());
    let check = Ident::new(&format!("__visiting_anc_{nonce}_check"), Span::call_site());
    let per: Vec<Ident> = (0..ancestors.len())
        .map(|i| Ident::new(&format!("__visiting_anc_{nonce}_{i}"), Span::call_site()))
        .collect();
    // Re-exported beside `impl_chain`, under names without the nonce, so the body can reach them as
    // `<module>::__visiting_anc_0!`. A bare name would do for a caller in this crate — a `#[macro_export]`
    // macro is in its own crate's textual scope — but not for one in another crate, and `$crate` here
    // means this crate. The module path the caller passes is the only spelling that works everywhere.
    let per_pub: Vec<Ident> = (0..ancestors.len())
        .map(|i| Ident::new(&format!("__visiting_anc_{i}"), Span::call_site()))
        .collect();
    let check_pub = Ident::new("__visiting_anc_check", Span::call_site());
    // Per declared type: the two method names and the alias the argument's type is named by. The
    // alias, not the type's own path, because the body is expanded in whatever crate invokes
    // `impl_chain!` and a `crate::`-rooted path would resolve there.
    let decl_parts = |a: &Ancestor, hops: &[Ident]| -> Vec<(Ident, Ident, TokenStream)> {
        a.declares
            .iter()
            // A method-generic one is quantified per call, and a muncher arm cannot be: it would
            // have to name the type parameters in the signature it writes. `#mg` reports it.
            .filter(|d| !d.method_generic)
            .map(|d| {
                let alias = Ident::new(&format!("__visiting_ty_{}", d.ident), Span::call_site());
                let args = &d.args;
                (
                    Side::SHARED.method(&d.ident),
                    Side::MUT.method(&d.ident),
                    quote!( $($vm)::+ #(:: #hops)* :: #alias #args ),
                )
            })
            .collect()
    };
    let munchers: Vec<TokenStream> = ancestors
        .iter()
        .zip(&per)
        .enumerate()
        .map(|(i, (a, m))| {
            let (g_params, g_use) = (&a.g_params, &a.g_use);
            let me = &per_pub[i];
            let hops = &a.relay;
            let parts = decl_parts(a, hops);
            quote! {
                #[macro_export]
                #[doc(hidden)]
                macro_rules! #m {
                    #(for (sh, mt, ty) in &parts) {
                        // Argument type left off: supply it.
                        ($($vm:ident)::+; $ty:ty; [$($k:tt)*] [$($km:tt)*] [$($s:tt)*]
                         $(#[$at:meta])* fn #sh (&mut $slf:ident, $i:ident) $b:block
                         $($rest:tt)*) => {
                            $($vm)::+ ::#chain_mod::#me!($($vm)::+; $ty;
                                [$($k)* $(#[$at])* fn #sh (&mut $slf, $i: & #ty) $b]
                                [$($km)*] [$($s)*] $($rest)*);
                        };
                        ($($vm:ident)::+; $ty:ty; [$($k:tt)*] [$($km:tt)*] [$($s:tt)*]
                         $(#[$at:meta])* fn #mt (&mut $slf:ident, $i:ident) $b:block
                         $($rest:tt)*) => {
                            $($vm)::+ ::#chain_mod::#me!($($vm)::+; $ty; [$($k)*]
                                [$($km)* $(#[$at])* fn #mt (&mut $slf, $i: &mut #ty) $b]
                                [$($s)*] $($rest)*);
                        };
                        // Written out in full.
                        ($($vm:ident)::+; $ty:ty; [$($k:tt)*] [$($km:tt)*] [$($s:tt)*]
                         $(#[$at:meta])* fn #sh ($($sig:tt)*) $(-> $rt:ty)? $b:block $($rest:tt)*) => {
                            $($vm)::+ ::#chain_mod::#me!($($vm)::+; $ty;
                                [$($k)* $(#[$at])* fn #sh ($($sig)*) $(-> $rt)? $b]
                                [$($km)*] [$($s)*] $($rest)*);
                        };
                        ($($vm:ident)::+; $ty:ty; [$($k:tt)*] [$($km:tt)*] [$($s:tt)*]
                         $(#[$at:meta])* fn #mt ($($sig:tt)*) $(-> $rt:ty)? $b:block $($rest:tt)*) => {
                            $($vm)::+ ::#chain_mod::#me!($($vm)::+; $ty; [$($k)*]
                                [$($km)* $(#[$at])* fn #mt ($($sig)*) $(-> $rt)? $b]
                                [$($s)*] $($rest)*);
                        };
                        // Handed back: this ancestor is the caller's to write.
                        ($($vm:ident)::+; $ty:ty; [$($k:tt)*] [$($km:tt)*] [$($s:tt)*]
                         fn #sh ; $($rest:tt)*) => {
                            $($vm)::+ ::#chain_mod::#me!($($vm)::+; $ty; [$($k)*] [$($km)*] [$($s)* mine]
                                $($rest)*);
                        };
                        ($($vm:ident)::+; $ty:ty; [$($k:tt)*] [$($km:tt)*] [$($s:tt)*]
                         fn #mt ; $($rest:tt)*) => {
                            $($vm)::+ ::#chain_mod::#me!($($vm)::+; $ty; [$($k)*] [$($km)*] [$($s)* mine]
                                $($rest)*);
                        };
                    }
                    // Someone else's.
                    ($($vm:ident)::+; $ty:ty; [$($k:tt)*] [$($km:tt)*] [$($s:tt)*]
                     $(#[$at:meta])* fn $other:ident ($($sig:tt)*) $(-> $rt:ty)? $b:block
                     $($rest:tt)*) => {
                        $($vm)::+ ::#chain_mod::#me!($($vm)::+; $ty; [$($k)*] [$($km)*] [$($s)*] $($rest)*);
                    };
                    ($($vm:ident)::+; $ty:ty; [$($k:tt)*] [$($km:tt)*] [$($s:tt)*]
                     fn $other:ident ; $($rest:tt)*) => {
                        $($vm)::+ ::#chain_mod::#me!($($vm)::+; $ty; [$($k)*] [$($km)*] [$($s)*] $($rest)*);
                    };
                    ($($vm:ident)::+; $ty:ty; [$($k:tt)*] [$($km:tt)*] []) => {
                        impl< #(#g_params,)* > $($vm)::+ #(:: #hops)* ::Visit #g_use for $ty {
                            $($k)*
                        }
                        impl< #(#g_params,)* > $($vm)::+ #(:: #hops)* ::VisitMut #g_use for $ty {
                            $($km)*
                        }
                    };
                    ($($vm:ident)::+; $ty:ty; [$($k:tt)*] [$($km:tt)*] [$($s:tt)+]) => {};
                }
            }
        })
        .collect();
    let all: Vec<Ident> = ancestors
        .iter()
        .flat_map(|a| a.declares.iter())
        .filter(|d| !d.method_generic)
        .flat_map(|d| Side::both().map(|side| side.method(&d.ident)))
        .collect();
    // Declared, but not writable here: the method takes its own type parameters. Accepting it would
    // make every muncher drop it and the override silently do nothing; the bare "no ancestor
    // declares it" message would be wrong, since one does.
    let mg_all: Vec<Ident> = ancestors
        .iter()
        .flat_map(|a| a.declares.iter())
        .filter(|d| d.method_generic)
        .flat_map(|d| Side::both().map(|side| side.method(&d.ident)))
        .collect();
    quote! {
        #(#munchers)*

        // A method no ancestor declares would be dropped by every muncher and quietly do nothing.
        #[macro_export]
        #[doc(hidden)]
        macro_rules! #check {
            #(
                ($($vm:ident)::+; $(#[$at:meta])* fn #all ($($sig:tt)*) $(-> $rt:ty)? $b:block
                 $($rest:tt)*) => {
                    $($vm)::+ ::#chain_mod::#check_pub!($($vm)::+; $($rest)*);
                };
                ($($vm:ident)::+; fn #all ; $($rest:tt)*) => {
                    $($vm)::+ ::#chain_mod::#check_pub!($($vm)::+; $($rest)*);
                };
            )*
            #(
                ($($vm:ident)::+; $(#[$at:meta])* fn #mg_all $($rest:tt)*) => {
                    ::core::compile_error!(::core::concat!(
                        "`", ::core::stringify!(#mg_all),
                        "` takes its own type parameters, so `impl_chain!` cannot write it: \
                         nothing it generates can be `for<T>` generic. Its node's parameters are \
                         not its visitor's, which is what makes the method generic. Write that \
                         ancestor's `impl Visit` out by hand instead."
                    ));
                };
            )*
            ($($vm:ident)::+; $(#[$at:meta])* fn $other:ident $($rest:tt)*) => {
                ::core::compile_error!(::core::concat!(
                    "`", ::core::stringify!($other),
                    "` is not a method of any visitor this one extends; \
                     a method of its own trait belongs in your `impl Visit`"
                ));
            };
            ($($vm:ident)::+;) => {};
        }

        #[macro_export]
        #[doc(hidden)]
        macro_rules! #name {
            ($($vm:ident)::+; $ty:ty) => { #name!($($vm)::+; $ty;); };
            ($($vm:ident)::+; $ty:ty; $($body:tt)*) => {
                $($vm)::+ ::#chain_mod::#check_pub!($($vm)::+; $($body)*);
                #( $($vm)::+ ::#chain_mod::#per_pub!($($vm)::+; $ty; [] [] [] $($body)*); )*
            };
        }
        // A glob import of the visitor module must bring in only what a user writes by hand, so the
        // chain machinery is reached through a module of its own rather than re-exported beside the
        // traits. A glob takes the module's *name*, not its contents.
        #[doc(hidden)]
        pub mod #chain_mod {
            #(
                #[doc(hidden)]
                pub use #per as #per_pub;
            )*
            #[doc(hidden)]
            pub use #check as #check_pub;
            #[doc(hidden)]
            pub use #name as impl_chain;
        }
    }
}

/// Whether a path is rooted in the *current* crate (`crate::` / `self::` / `super::`). A foreign path
/// (an external crate name, or a leading `::`) is not — an inherent `impl` for such a type would be
/// E0116 (inherent impls must live in the type's defining crate), so the visitor skips the inherent
/// `.visit()`/`.visit_mut()` for a foreign target and the trait method (`Visit::visit_*`) is used.
pub(crate) fn path_is_crate_local(p: &Path) -> bool {
    if p.leading_colon.is_some() {
        return false;
    }
    matches!(
        p.segments.first().map(|s| s.ident.to_string()).as_deref(),
        Some("crate") | Some("self") | Some("super")
    )
}

/// The host crate of a direct-base path: `Some(ident)` when it is rooted at an *external* crate —
/// `other_crate::inherit::mid`, or `::other_crate::inherit::mid`, which names the same crate and is
/// equally external — and `None` for a same-crate root (`crate`/`super`/`self`). Gates the ancestor requalification (`requalify_ancestor`): a transitive
/// ancestor an *upstream* intermediate recorded relative to its own crate must be rewritten into a
/// path the *downstream* extender can resolve. (A `$crate` cannot do this: emitted by a proc-macro
/// into a generated `macro_rules` body it resolves only for fetch/macro-invocation paths, **not** for
/// the trait path re-emitted into the new `Driver`'s supertrait impl — so a cross-crate `base => mid
/// => new` with an *upstream* `mid` needs this concrete requalification instead.)
pub(crate) fn base_host_crate(base: &Path) -> Option<Ident> {
    let first = base.segments.first()?;
    if !matches!(first.arguments, PathArguments::None) {
        return None;
    }
    let s = first.ident.to_string();
    if s == "crate" || s == "super" || s == "self" {
        None
    } else {
        Some(first.ident.clone())
    }
}

/// The prefix each visitor module re-exports one of its bases under, so an ancestor is reachable
/// through the chain rather than by its own path. A base module that is private, or that a
/// downstream crate cannot otherwise name, is still reachable as `<base>::__visiting_base_0`.
///
/// Indexed, because a visitor may extend several: base `k` is relayed under
/// `__visiting_base_<k>`, and an ancestor is a sequence of such hops rather than a depth.
pub(crate) const BASE_RELAY: &str = "__visiting_base";

/// The relay module name for base `k`.
pub(crate) fn base_relay_name(k: usize) -> Ident {
    Ident::new(&format!("{BASE_RELAY}_{k}"), Span::call_site())
}

/// The submodule each visitor module keeps its chain macros in. They are callable — the tests and
/// any generated code reach them as `<module>::__visiting_chain::impl_chain!` — but a glob import of the
/// visitor module takes this name alone, not what is inside it.
pub(crate) const CHAIN_MOD: &str = "__visiting_chain";

/// `base` followed by `hops`, the `__visiting_base_<k>` re-exports: no hops is `base` itself.
///
/// This is what lets an ancestor chain cross a crate boundary without any path arithmetic: every
/// link is named relative to the one below it, so no segment of it has to be nameable from the
/// extending crate. (Contrast `requalify_ancestor`, which rewrites a recorded path and therefore
/// needs every module on it to be public.)
pub(crate) fn relay_path(base: &Path, hops: &[Ident]) -> Path {
    let mut p = base.clone();
    for h in hops {
        p.segments.push(PathSegment {
            ident: h.clone(),
            arguments: PathArguments::None,
        });
    }
    p
}

/// Resolve a transitive ancestor path that an *upstream* intermediate recorded **relative to its own
/// module** into one the *downstream* extender can resolve, using the direct `base` path. Downstream,
/// `base` is the path the extender named the intermediate by (e.g. `other_crate::inherit::mid_ss`), and
/// the intermediate's `define_visit!()` was invoked *inside that module* — so the ancestor's leading
/// relative segment resolves against `base`:
///   - `crate::REST` → `<host>::REST`   (host = `base`'s first segment, the upstream crate)
///   - `super::REST` → pop one trailing segment of `base` per leading `super`, then append `REST`
///   - `self::REST`  → `base::REST`
///
/// A path already concrete (external-crate-rooted or leading `::`) — or a bare ident — is left alone.
/// Only called for a cross-crate base (`base_host_crate(base).is_some()`); same-crate chains, which
/// resolve in place, keep their recorded paths. This is why a `super`/`self`-relative `define_visit!` entry
/// path (not just the canonical `crate::`-rooted one) now works cross-crate.
pub(crate) fn requalify_ancestor(anc: &Path, base: &Path) -> Path {
    if anc.leading_colon.is_some() {
        return anc.clone();
    }
    let first = match anc.segments.first() {
        Some(first) => first,
        None => return anc.clone(),
    };
    if !matches!(first.arguments, PathArguments::None) {
        return anc.clone();
    }
    // `base`'s segments ARE the intermediate's module path (the `define_visit!()` ran inside it).
    let base_mod: Vec<PathSegment> = base.segments.iter().cloned().collect();
    let join = |prefix: &[PathSegment], tail: &[PathSegment]| -> Path {
        let mut segments = Punctuated::new();
        for s in prefix.iter().chain(tail.iter()) {
            segments.push(s.clone());
        }
        Path {
            leading_colon: None,
            segments,
        }
    };
    match first.ident.to_string().as_str() {
        "crate" => {
            let mut out = anc.clone();
            out.segments[0].ident = base_mod[0].ident.clone();
            out
        }
        "self" => {
            let tail: Vec<PathSegment> = anc.segments.iter().skip(1).cloned().collect();
            join(&base_mod, &tail)
        }
        "super" => {
            let supers = anc
                .segments
                .iter()
                .take_while(|s| s.ident == "super" && matches!(s.arguments, PathArguments::None))
                .count();
            let keep = base_mod.len().saturating_sub(supers);
            let tail: Vec<PathSegment> = anc.segments.iter().skip(supers).cloned().collect();
            join(&base_mod[..keep], &tail)
        }
        _ => anc.clone(),
    }
}

/// Parse `@done { @t { @path {..} @def {..} @subast {..} } .. }`.
fn parse_done(ts: TokenStream) -> Result<Vec<DoneType>> {
    let parser = |input: ParseStream| {
        let mut out = Vec::new();
        while !input.is_empty() {
            input.parse::<Token![@]>()?;
            let kw: Ident = input.parse()?;
            if kw != "t" {
                return Err(Error::new(kw.span(), "expected `@t` in @done"));
            }
            let content;
            braced!(content in input);
            out.push(parse_done_type(&content)?);
        }
        Ok(out)
    };
    parser.parse2(ts)
}

fn parse_done_type(input: ParseStream) -> Result<DoneType> {
    let mut path = None;
    let mut def = None;
    let mut subast = Vec::new();
    while !input.is_empty() {
        let (name, content) = parse_section(input)?;
        match name.to_string().as_str() {
            "path" => path = Some(syn::parse2(content)?),
            "def" => def = Some(syn::parse2(content)?),
            "subast" => subast = parse_subentries(content)?,
            other => {
                return Err(Error::new(
                    name.span(),
                    format!("unknown @t section @{other}"),
                ))
            }
        }
    }
    Ok(DoneType {
        path: path.ok_or_else(|| Error::new(Span::call_site(), "missing @path in @t"))?,
        def: def.ok_or_else(|| Error::new(Span::call_site(), "missing @def in @t"))?,
        subast,
    })
}

impl Parse for BuildInput {
    fn parse(input: ParseStream) -> Result<Self> {
        // `@bases`/`@inherited`/`@anc` are the carried, already-resolved state. `@inh`/`@bg`/`@an`/
        // `@own`/`@sz`/`@id` are what the base just asked answers with, and are folded in after the
        // loop, once `@bnow` says which base they belong to.
        let mut bases: Vec<BaseIn> = Vec::new();
        let mut base_fetch: Vec<Path> = Vec::new();
        let mut base_now: Option<Path> = None;
        let mut fresh_inh: Vec<SubEntry> = Vec::new();
        let mut fresh_bg: Vec<GenericParam> = Vec::new();
        let mut fresh_an: Vec<AncIn> = Vec::new();
        let mut fresh_own: Vec<Decl> = Vec::new();
        let mut fresh_sz = false;
        let mut fresh_id = TokenStream::new();
        let mut fresh_up: Vec<TokenStream> = Vec::new();
        let mut build = None;
        let mut nonce = TokenStream::new();
        let mut visited = Vec::new();
        let mut inherited = Vec::new();
        let mut base_ancestors = Vec::new();
        let mut fetching = None;
        let mut done = Vec::new();
        let mut rest = Vec::new();
        let mut just_def = None;
        let mut just_subast = Vec::new();

        while !input.is_empty() {
            let (name, content) = parse_section(input)?;
            match name.to_string().as_str() {
                "bases" => bases = parse_bases(content)?,
                "bfetch" => {
                    base_fetch = Punctuated::<Path, Token![,]>::parse_terminated
                        .parse2(content)?
                        .into_iter()
                        .collect();
                }
                "bnow" => {
                    base_now = Punctuated::<Path, Token![,]>::parse_terminated
                        .parse2(content)?
                        .into_iter()
                        .next();
                }
                "build" => build = Some(syn::parse2(content)?),
                "nonce" => nonce = content,
                "visited" => {
                    visited = Punctuated::<Path, Token![,]>::parse_terminated
                        .parse2(content)?
                        .into_iter()
                        .collect();
                }
                "inherited" => inherited.extend(parse_subentries(content)?),
                "inh" => fresh_inh = parse_subentries(content)?,
                "bg" => {
                    if !content.is_empty() {
                        fresh_bg = Punctuated::<GenericParam, Token![,]>::parse_terminated
                            .parse2(content)?
                            .into_iter()
                            .collect();
                    }
                }
                "anc" => base_ancestors = parse_ancestors(content)?,
                "an" => fresh_an = parse_ancestors(content)?,
                "own" => fresh_own = parse_decls(content)?,
                "sz" => fresh_sz = !content.is_empty(),
                "id" => fresh_id = content,
                "up" => fresh_up = parse_ids(content)?,
                "fetching" => {
                    if !content.is_empty() {
                        fetching = Some(syn::parse2(content)?);
                    }
                }
                "done" => done = parse_done(content)?,
                "rest" => {
                    let paths = Punctuated::<Path, Token![,]>::parse_terminated.parse2(content)?;
                    rest = paths.into_iter().collect();
                }
                "ast" => just_def = Some(syn::parse2(content)?),
                "subast" => just_subast = parse_subentries(content)?,
                other => return Err(Error::new(name.span(), format!("unknown section @{other}"))),
            }
        }

        // Resolve the reply against the base that gave it, so the merged lists stand on their own
        // and nothing downstream has to remember which base an entry came from. The inherited *type*
        // paths are requalified; the ancestor *module* paths are reached through this base's hop, so
        // no segment of them has to be nameable here. A module two bases both reach — a diamond — is
        // kept once, told by identity rather than by path, since each base names it its own way.
        if let Some(bn) = &base_now {
            let hop = base_relay_name(bases.len());
            let seen: HashSet<String> = inherited.iter().map(|e| norm_path(&e.path)).collect();
            for e in fresh_inh {
                let path = if needs_requalify(&e.path, bn) {
                    requalify_ancestor(&e.path, bn)
                } else {
                    e.path.clone()
                };
                if !seen.contains(&norm_path(&path)) {
                    inherited.push(SubEntry { path, key: e.key });
                }
            }
            let mut known: HashSet<String> = base_ancestors
                .iter()
                .map(|a| a.id.to_string())
                .chain(bases.iter().map(|b| b.id.to_string()))
                .collect();
            if known.insert(fresh_id.to_string()) {
                bases.push(BaseIn {
                    path: bn.clone(),
                    generics: fresh_bg,
                    own: fresh_own,
                    sized_self: fresh_sz,
                    id: fresh_id,
                    up: fresh_up,
                });
                for a in fresh_an {
                    if !known.insert(a.id.to_string()) {
                        continue;
                    }
                    let mut relay = vec![hop.clone()];
                    relay.extend(a.relay);
                    base_ancestors.push(AncIn {
                        path: relay_path(bn, &relay[1..]),
                        relay,
                        names: a.names,
                        declares: a.declares,
                        id: a.id,
                        up: a.up,
                    });
                }
            }
        }

        Ok(BuildInput {
            bases,
            base_fetch,
            build: build.ok_or_else(|| Error::new(Span::call_site(), "missing @build"))?,
            nonce,
            visited,
            inherited,
            base_ancestors,
            fetching,
            done,
            rest,
            just_def,
            just_subast,
        })
    }
}

/// `@bases { @b { @p {PATH} @g {params} @d {decls} @sz {yes?} @i {id} @u {{id}…} } … }` — the bases
/// whose replies have already arrived.
pub(crate) fn emit_bases(bases: &[BaseIn]) -> TokenStream {
    let blocks: Vec<TokenStream> = bases
        .iter()
        .map(|b| {
            let (path, generics, id) = (&b.path, &b.generics, &b.id);
            let decls = emit_decls(&b.own);
            let up = emit_ids(&b.up);
            quote! {
                @b {
                    @p { #path } @g { #(#generics),* } @d { #decls }
                    @sz { #(if b.sized_self) { yes } } @i { #id } @u { #up }
                }
            }
        })
        .collect();
    quote!( #(#blocks)* )
}

fn parse_bases(ts: TokenStream) -> Result<Vec<BaseIn>> {
    let parser = |input: ParseStream| {
        let mut out = Vec::new();
        while !input.is_empty() {
            input.parse::<Token![@]>()?;
            let kw: Ident = input.parse()?;
            if kw != "b" {
                return Err(Error::new(kw.span(), "expected `@b` in @bases"));
            }
            let content;
            braced!(content in input);
            let mut path = None;
            let mut generics = Vec::new();
            let mut own = Vec::new();
            let mut sized_self = false;
            let mut id = TokenStream::new();
            let mut up = Vec::new();
            while !content.is_empty() {
                let (name, inner) = parse_section(&content)?;
                match name.to_string().as_str() {
                    "p" => path = Some(syn::parse2(inner)?),
                    "g" => {
                        if !inner.is_empty() {
                            generics = Punctuated::<GenericParam, Token![,]>::parse_terminated
                                .parse2(inner)?
                                .into_iter()
                                .collect();
                        }
                    }
                    "d" => own = parse_decls(inner)?,
                    "sz" => sized_self = !inner.is_empty(),
                    "i" => id = inner,
                    "u" => up = parse_ids(inner)?,
                    other => {
                        return Err(Error::new(
                            name.span(),
                            format!("unknown @b section @{other}"),
                        ))
                    }
                }
            }
            out.push(BaseIn {
                path: path.ok_or_else(|| Error::new(Span::call_site(), "missing @p in @b"))?,
                generics,
                own,
                sized_self,
                id,
                up,
            });
        }
        Ok(out)
    };
    parser.parse2(ts)
}

/// `{ {id} {id} … }` — a list of visitor identities, each braced so an id may be any token run.
fn emit_ids(ids: &[TokenStream]) -> TokenStream {
    quote!( #({ #ids })* )
}

fn parse_ids(ts: TokenStream) -> Result<Vec<TokenStream>> {
    let parser = |input: ParseStream| {
        let mut out = Vec::new();
        while !input.is_empty() {
            let inner;
            braced!(inner in input);
            out.push(inner.parse()?);
        }
        Ok(out)
    };
    parser.parse2(ts)
}

fn parse_idents(ts: TokenStream) -> Result<Vec<Ident>> {
    let parser = |input: ParseStream| {
        let mut out = Vec::new();
        while !input.is_empty() {
            out.push(input.parse::<Ident>()?);
        }
        Ok(out)
    };
    parser.parse2(ts)
}

/// Serialize one `__visitor_build` ping-pong bounce's full state payload. Shared by `entry` (the
/// first bounce — `inherited`/`anc`/`done` are always empty, nothing fetched and no base asked yet)
/// and `build` (every later bounce, carrying the accumulated state). Content pieces that need their
/// own rendering (`@bases`, `@anc`, `@done`) are passed pre-rendered so this fn stays a pure
/// section-list assembler. `@bnow` names the base whose reply will trail this payload, `@fetching`
/// the type whose definition will — never both.
#[allow(clippy::too_many_arguments)]
pub(crate) fn state_tokens(
    bases: &TokenStream, // emit_bases(&bases)
    bfetch: &[Path],
    bnow: &TokenStream,
    build: &Path,
    nonce: &TokenStream,
    visited: &[Path],
    inherited: &[SubEntry],
    anc: &TokenStream, // emit_ancestors(&base_ancestors) or quote!()
    fetching: &TokenStream,
    done: &TokenStream, // emit_done(&done) or quote!()
    rest: &[Path],
) -> TokenStream {
    quote! {
        @bases { #bases }
        @bfetch { #(#bfetch),* }
        @bnow { #bnow }
        @build { #build }
        @nonce { #nonce }
        @visited { #(#visited),* }
        @inherited { #(for e in inherited), { #{&e.path} as #{&e.key} } }
        @anc { #anc }
        @fetching { #fetching }
        @done { #done }
        @rest { #(#rest),* }
    }
}

/// The accumulated state, ready for the next bounce.
pub(crate) fn carry(st: &BuildInput, fetching: &TokenStream, bnow: &TokenStream) -> TokenStream {
    state_tokens(
        &emit_bases(&st.bases),
        &st.base_fetch,
        bnow,
        &st.build,
        &st.nonce,
        &st.visited,
        &st.inherited,
        &emit_ancestors(&st.base_ancestors),
        fetching,
        &emit_done(&st.done),
        &st.rest,
    )
}

/// `__visitor_build` is an `announce` callback as well as the carrier of its own state.
///
/// A fetch invokes the target's trampoline with this macro as the last continuation, so what comes
/// back is announce's shape — the continuation list, the definitions newest-first, then our state as
/// announce's untouched `REST`, behind an `@state` marker. The definition and the follow-list are
/// read out of the entry; the follow-list rides in announce's `META` as `visiting(..)`, written
/// either by our own derive or by a `visiting(..)` check the definer passed to `#[announce]`. A type
/// announced by a crate that has never heard of this one carries none, which is right: it is a leaf.
///
/// The first bounce carries only state, with no announce wrapper, so both shapes are accepted.
fn parse_callback_or_state(input: TokenStream) -> syn::Result<BuildInput> {
    /// Peel `@state <callback> { <state> }`, which is what we put in announce's `REST`.
    fn unwrap_state(ts: TokenStream) -> syn::Result<TokenStream> {
        fn p(input: syn::parse::ParseStream) -> syn::Result<TokenStream> {
            input.parse::<Token![@]>()?;
            let kw: Ident = input.parse()?;
            if kw != "state" {
                return Err(syn::Error::new(kw.span(), "expected `@state`"));
            }
            input.parse::<Path>()?;
            Ok(input.parse::<proc_macro2::Group>()?.stream())
        }
        (p as fn(syn::parse::ParseStream) -> syn::Result<TokenStream>).parse2(ts)
    }

    if let Ok(ann) = announce_core::helper::Input::parse.parse2(input.clone()) {
        if let Ok(state_ts) = unwrap_state(ann.rest.clone()) {
            let mut st: BuildInput = syn::parse2(state_ts)?;
            if let Some(entry) = ann.entries.first() {
                let def = entry.item()?;
                // The follow-list arrives by either of two routes, and a node may use both.
                // `visiting(..)` in META is what our derive renders (resolved) and what a definer
                // wrote in the check's arguments — `find_meta` compares last segments, so the
                // check's own `visiting::visiting(..)` matches the same name. `#[visiting(..)]`
                // *inside* the definition is the third: an attribute the check owns, which announce
                // keeps in the payload.
                if let Some(m) = entry.find_meta("visiting") {
                    if let Some(args) = m.args.clone() {
                        st.just_subast = parse_follow_meta(args)?;
                    }
                }
                st.just_subast.extend(parse_follow_attrs(&def)?);
                st.just_def = Some(def);
            }
            return Ok(st);
        }
    }
    syn::parse2(input)
}

pub fn build(input: TokenStream) -> TokenStream {
    let mut st: BuildInput = match parse_callback_or_state(input) {
        Ok(s) => s,
        Err(e) => return e.to_compile_error(),
    };

    if let Some(def) = st.just_def.take() {
        let path = match st.fetching.clone() {
            Some(p) => p,
            None => {
                return Error::new(Span::call_site(), "internal: @ast without @fetching")
                    .to_compile_error()
            }
        };
        let subast = std::mem::take(&mut st.just_subast);

        let method_set = st.method_set();
        let self_ident = item_ident(&def);
        let mut seen: HashSet<String> = st
            .done
            .iter()
            .map(|d| norm_path(&d.path))
            .chain(st.rest.iter().map(norm_path))
            .collect();
        for entry_path in
            followed_intermediates(&def, &subast, &method_set, self_ident, &method_set)
        {
            if seen.insert(norm_path(&entry_path)) {
                st.rest.push(entry_path);
            }
        }
        st.done.push(DoneType { path, def, subast });
    }
    st.fetching = None;

    // Ask the next base for its metadata before any type is fetched, so the inherited set is
    // complete by the time a field is lowered against it.
    if !st.base_fetch.is_empty() {
        let next = st.base_fetch.remove(0);
        let state = carry(&st, &quote!(), &quote!(#next));
        let build = &st.build;
        return quote! { #next::__visiting_visited ! { @visited #build { #state } } };
    }

    if !st.rest.is_empty() {
        let next = st.rest.remove(0);
        let state = carry(&st, &quote!(#next), &quote!());
        let build = &st.build;
        // announce's calling convention; our state rides in `REST`.
        return quote! { #next ! { [ #next !, #build ! ] @state #build { #state } } };
    }

    generate_module(&st)
}

/// Re-serialize `@done` (the fetched types) for the next ping-pong bounce.
fn emit_done(done: &[DoneType]) -> TokenStream {
    let blocks: Vec<TokenStream> = done
        .iter()
        .map(|d| {
            let path = &d.path;
            let def = &d.def;
            let subast = subentries_tokens(&d.subast);
            quote! { @t { @path { #path } @def { #def } @subast { #subast } } }
        })
        .collect();
    quote!( #(#blocks)* )
}

/// Whether a path recorded by `base`'s visitor has to be rewritten before this module can use it.
///
/// `self::`/`super::` are relative to the *base's* module, which is never this one — always rewrite,
/// or a base and an extender at different nesting depths silently name different types. `crate::`
/// already names the right crate when the base lives in this one, and must be re-rooted only when it
/// does not. An absolute or external-crate path needs nothing.
pub(crate) fn needs_requalify(p: &Path, base: &Path) -> bool {
    if p.leading_colon.is_some() {
        return false;
    }
    match p.segments.first().map(|s| s.ident.to_string()).as_deref() {
        Some("self") | Some("super") => true,
        Some("crate") => base_host_crate(base).is_some(),
        _ => false,
    }
}
