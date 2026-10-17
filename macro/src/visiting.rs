use crate::util::{data_field_iter, for_each_type_path};
use proc_macro2::{Literal, TokenStream};
use proc_macro_error3::{abort, emit_error};
use std::collections::{HashMap, HashSet};
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::*;
use template_quote::quote;

/// One `#[visiting(..)]` entry: a path to a sub-AST type, optionally aliased (`b::Foo as BFoo`). The
/// `matchkey` (the alias, or the path's last segment) is the ident a field head is matched against;
/// `path` is the resolvable path used to fetch that sub-AST's metadata macro / as a drill scrutinee.
pub(crate) struct SubastEntry {
    pub(crate) path: Path,
    pub(crate) alias: Option<Ident>,
}

impl Parse for SubastEntry {
    fn parse(input: ParseStream) -> Result<Self> {
        let path: Path = input.parse()?;
        let alias = if input.peek(Token![as]) {
            input.parse::<Token![as]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        Ok(SubastEntry { path, alias })
    }
}

impl SubastEntry {
    /// The ident a (container-peeled) field head is matched against.
    pub(crate) fn matchkey(&self) -> Ident {
        self.alias
            .clone()
            .unwrap_or_else(|| self.path.segments.last().unwrap().ident.clone())
    }
}

/// Whether a `#[visiting(..)]` path is **fully qualified (rooted)** — so it still resolves when this
/// type's metadata macro is expanded elsewhere (a sibling module, or a downstream crate building a
/// visitor that drills through it). Rooted forms: a leading `::` (absolute), `crate::…` (re-rooted to
/// `$crate::…` for downstream), or an external-crate path (`other_crate::…`) — i.e. a multi-segment path
/// whose first segment is not `self`/`super`. NOT rooted: a bare single-segment ident, or a
/// `self::`/`super::`-relative path — those resolve in the *consumer's* context, not the definition's.
pub(crate) fn subast_path_is_rooted(path: &Path) -> bool {
    if path.leading_colon.is_some() {
        return true;
    }
    match path.segments.first() {
        Some(seg) if seg.ident == "self" || seg.ident == "super" => false,
        _ => path.segments.len() >= 2,
    }
}

/// One item inside `#[visiting(..)]`: either `crate = <path>`, which names the facade crate the
/// generated code should reach this crate through, or a path to a type the walk should follow.
enum VisitingItem {
    Crate(Path),
    Follow(SubastEntry),
}

impl Parse for VisitingItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Token![crate]) && input.peek2(Token![=]) {
            input.parse::<Token![crate]>()?;
            input.parse::<Token![=]>()?;
            return Ok(VisitingItem::Crate(input.parse()?));
        }
        Ok(VisitingItem::Follow(input.parse()?))
    }
}

/// The contents of one `#[visiting(..)]`: a facade crate, types to follow, or both.
///
/// One attribute does both jobs, told apart by syntax — `crate = <path>` names the crate the
/// generated code should reach this one through, and a bare path names a type the walk should
/// follow. [`merge`](Self::merge) folds a second attribute's contents in, so the two forms may share
/// one attribute or sit on separate ones.
#[derive(Default)]
pub(crate) struct VisitingAttribute {
    pub(crate) krate: Option<Path>,
    pub(crate) follow: Vec<SubastEntry>,
}

impl Parse for VisitingAttribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut args = VisitingAttribute::default();
        for item in Punctuated::<VisitingItem, Token![,]>::parse_terminated(input)? {
            match item {
                // First one wins; a second is a mistake worth naming rather than silently dropping.
                VisitingItem::Crate(p) => match &args.krate {
                    None => args.krate = Some(p),
                    Some(_) => emit_error!(p, "`crate = ..` given more than once"),
                },
                VisitingItem::Follow(e) => args.follow.push(e),
            }
        }
        Ok(args)
    }
}

impl VisitingAttribute {
    fn merge(&mut self, other: VisitingAttribute) {
        match (&self.krate, other.krate) {
            (None, k) => self.krate = k,
            (Some(_), Some(p)) => emit_error!(p, "`crate = ..` given more than once"),
            (Some(_), None) => {}
        }
        self.follow.extend(other.follow);
    }
}

/// Read every `#[visiting(..)]` on the type, in one pass.
///
/// Parsing only: the caller validates the list it ends up with ([`validate_follow`]), since the
/// check merges this with a list of its own and a complaint should be made once.
pub(crate) fn parse_visiting(attrs: &[Attribute]) -> VisitingAttribute {
    let mut args = VisitingAttribute::default();
    for attr in attrs {
        if !attr.path().is_ident("visiting") {
            continue;
        }
        let list = match &attr.meta {
            Meta::List(ml) => ml.tokens.clone(),
            _ => {
                emit_error!(
                    attr,
                    "`#[visiting(..)]` takes a parenthesized list of paths"
                );
                continue;
            }
        };
        match VisitingAttribute::parse.parse2(list) {
            Ok(parsed) => args.merge(parsed),
            Err(e) => emit_error!(e.span(), "invalid `#[visiting(..)]`: {}", e),
        }
    }
    args
}

/// Check a follow-list: every path fully qualified, none carrying generic arguments, no two sharing a
/// matchkey. Shared by the `#[visiting(..)]` attribute and the `announce` check, which differ only in
/// where the list is written — the rules are about the list itself.
fn validate_follow(entries: &[SubastEntry]) {
    // Every entry must be rooted, whatever it names. The path has two jobs: as a *match key* only the
    // last segment is compared, so the root is irrelevant there — but as a *fetch target* for a type
    // `define_visit!(..)` does not list, it is invoked as a macro to pull that type's definition, in the
    // visitor's scope, where a bare or `self::`/`super::`-relative path means something else.
    //
    // The rule is unconditional rather than applied only to fetched entries, because which entries are
    // fetched depends on the `define_visit!(..)` list — so a conditional rule would accept an attribute
    // today and reject it when someone stops listing that type, and would report at the `define_visit!`
    // invocation rather than at the attribute. A `pub use` beside the definition could launder a
    // relative path (a `use` resolves where it is written), but only one level deep: past the first hop the alias
    // hides the module the next fetch needs, so the restriction would come back a level further from
    // the code that has to change.
    for e in entries {
        if !subast_path_is_rooted(&e.path) {
            let p = &e.path;
            emit_error!(
                e.path,
                "`visiting(..)` path `{}` is not fully qualified. Use a `crate`-rooted path \
                 (`crate::path::to::{}`) — or an external-crate path (`other_crate::path::{}`) for a \
                 type from another crate. A bare ident or a `self::`/`super::`-relative path resolves \
                 in the consumer's scope (where the metadata macro is expanded), not here.",
                quote!(#p).to_string().replace(' ', ""),
                e.matchkey(),
                e.matchkey(),
            );
        }
        // Reject generic arguments on any segment: a subast entry names a type by *path only* — the path
        // is used verbatim as a metadata-macro fetch target (`path! { .. }`) and a drill scrutinee, where
        // `<..>` is illegal (otherwise a cryptic "expected `!` or `::`, found `<`" surfaces at the visitor).
        if e.path
            .segments
            .iter()
            .any(|s| !matches!(s.arguments, PathArguments::None))
        {
            let p = &e.path;
            let mut stripped = e.path.clone();
            for s in &mut stripped.segments {
                s.arguments = PathArguments::None;
            }
            emit_error!(
                e.path,
                "`visiting(..)` path `{}` carries generic arguments; a follow-list entry names a type by \
                 path only (it is used as a metadata-macro fetch target and a match scrutinee, where \
                 `<..>` is illegal). Drop the arguments — write `{}`.",
                quote!(#p).to_string().replace(' ', ""),
                quote!(#stripped).to_string().replace(' ', ""),
            );
        }
    }
    // Two entries with the same matchkey cannot be told apart, but the fix depends on why they
    // collide: the same type named twice is a redundant entry to delete, whereas two *different*
    // types whose paths end alike need one of them aliased. Advising an alias for the first case
    // sends the user to write `crate::Leaf as Other`, which compiles and buys nothing.
    let mut seen: HashMap<String, Path> = HashMap::new();
    for e in entries {
        let key = e.matchkey().to_string();
        let spelling = bare_path(&e.path);
        match seen.get(&key) {
            None => {
                seen.insert(key, e.path.clone());
            }
            Some(first) if bare_path(first) == spelling => emit_error!(
                e.path,
                "`visiting(..)` names `{}` twice; remove the duplicate entry",
                spelling,
            ),
            Some(first) => emit_error!(
                e.path,
                "two `visiting(..)` entries share the last segment `{}` (`{}` and `{}`), and a \
                 field's head is matched by that segment alone, so they cannot be told apart. \
                 Alias one — `{} as SomeOtherName` — and write that name in the field.",
                key,
                bare_path(first),
                spelling,
                spelling,
            ),
        }
    }
}

/// Every ident that appears as a path segment anywhere inside a field type (so `Vec<Box<Stmt<S>>>`
/// contributes `Vec`, `Box`, `Stmt`, `S`). Used only to warn about `#[visiting]` entries that match no
/// field — an over-approximation, so it never false-warns.
fn field_head_idents(input: &DeriveInput) -> HashSet<String> {
    let mut out = HashSet::new();
    for_each_field_type_of(&input.data, &mut |ty| {
        for_each_type_path(ty, &mut |tp| {
            out.extend(tp.path.segments.iter().map(|s| s.ident.to_string()));
        })
    });
    out
}

/// Call `f` for every field type of a derive input's body.
fn for_each_field_type_of(data: &Data, f: &mut dyn FnMut(&Type)) {
    data_field_iter(data).for_each(|fld| f(&fld.ty));
}

/// A path with its generic arguments stripped, as a comparable spelling (`Vec<Box<T>>` → `Vec`).
fn bare_path(p: &Path) -> String {
    let lead = if p.leading_colon.is_some() { "::" } else { "" };
    let segs: Vec<String> = p.segments.iter().map(|s| s.ident.to_string()).collect();
    format!("{lead}{}", segs.join("::"))
}

/// Reject a definition whose fields spell two *different* paths ending in the same identifier.
///
/// A visitor recognises a field's head by its **last path segment**, so within one definition those
/// two are indistinguishable: whichever is the visited type, the other is matched as well. Catching it
/// here points at the definition that carries the ambiguity, rather than leaving it to surface as a
/// mis-resolved field — or, worse, as a field that silently stops being followed.
fn check_ambiguous_last_idents(input: &DeriveInput) {
    // Every path a field mentions, keyed by its last segment, keeping one occurrence of each distinct
    // spelling so a diagnostic can point at it.
    let mut by_last: HashMap<String, Vec<(String, Path)>> = HashMap::new();
    for_each_field_type_of(&input.data, &mut |ty| {
        for_each_type_path(ty, &mut |tp| {
            let bare = bare_path(&tp.path);
            let slot = by_last
                .entry(tp.path.segments.last().unwrap().ident.to_string())
                .or_default();
            if !slot.iter().any(|(s, _)| s == &bare) {
                slot.push((bare, tp.path.clone()));
            }
        })
    });
    for (last, spellings) in &by_last {
        if spellings.len() < 2 {
            continue;
        }
        let names: Vec<&str> = spellings.iter().map(|(s, _)| s.as_str()).collect();
        emit_error!(
            spellings[1].1,
            "this type's fields spell {} different paths ending in `{}` ({}); a visitor matches a \
             field's head by its last path segment, so they cannot be told apart",
            spellings.len(),
            last,
            names.join(", ");
            help = "give one a distinct name, or name it through a `#[visiting(<path> as Alias)]` \
                    entry and write that alias in the field"
        );
    }
}

/// Warn about a follow entry that matches no field of the definition.
///
/// `field_head_idents` over-approximates (it collects every segment of every field type), so this
/// never fires on an entry that is in fact reachable.
fn warn_unmatched_entries(input: &DeriveInput, subast: &[SubastEntry]) -> TokenStream {
    let field_heads = field_head_idents(input);
    let mut out = TokenStream::new();
    for e in subast {
        let key = e.matchkey();
        if !field_heads.contains(&key.to_string()) {
            out.extend(unmatched_entry_warning(e, &key, &input.ident));
        }
    }
    out
}

/// A warning rustc will actually print, for an entry that matches no field.
///
/// `emit_warning!` cannot be used: off nightly, proc-macro-error3 keeps only `Level::Error`
/// (`imp/fallback.rs`, `emit_diagnostic`) and renders a warning as an empty `TokenStream`, so the
/// diagnostic is silently dropped — which is what this used to do. There is no stable way for a
/// proc macro to raise a warning of its own, so the warning is emitted as *code rustc warns about*:
/// a deprecated item, used once. Both the item and its use are spanned at the entry, so the
/// `-->` lands on the path at fault rather than inside generated code.
///
/// Everything lives in an anonymous `const` block, so the marker type is not added to the scope the
/// definition sits in, and the whole thing disappears once the entry is removed or corrected.
fn unmatched_entry_warning(e: &SubastEntry, key: &Ident, ident: &Ident) -> TokenStream {
    // The last segment, not the whole path: `Path::span()` has no joined span to give on stable
    // and falls back to the first token, which would underline `crate`. The last segment is also
    // the matchkey the message is about.
    let span = e
        .path
        .segments
        .last()
        .map_or_else(|| e.path.span(), |s| s.ident.span());
    // The lint prints `use of deprecated struct `_::<name>`: <note>`, so the name is part of the
    // sentence the user reads.
    let name = Ident::new(&format!("unmatched_visiting_entry_{}", key), span);
    let note = Literal::string(&format!(
        "`visiting(..)` entry `{}` matches no field of `{}`, so nothing is ever walked through it. \
         Remove the entry, or correct its path to the type a field actually names (a field's head is \
         matched by last path segment, so an alias or a type parameter does not match).",
        key, ident,
    ));
    // The *use* is what the lint points at, so it is the token that has to carry the entry's span.
    let use_site = quote::quote_spanned!(span => #name);
    quote! {
        const _: () = {
            #[deprecated(note = #note)]
            #[allow(non_camel_case_types)]
            struct #name;
            let _: ::core::option::Option<#use_site> = ::core::option::Option::None;
        };
    }
}

/// One assertion per follow entry that the entry is announced too, so a visitor can fetch it.
fn follow_announced(input: &DeriveInput, subast: &[SubastEntry], root: &Path) -> TokenStream {
    let (impl_generics, _, where_clause) = input.generics.split_for_impl();

    // Every `#[visiting]` entry must itself be announced. Without this the omission surfaces only when
    // a `define_visit!` tries to expand the entry's trampoline, as `cannot find `X` in `$crate`` — a
    // message about a macro the user never wrote, in a module far from the attribute at fault.
    //
    // The bound names the matching FIELD's type, not the `#[visiting]` path: an entry is written bare
    // (`crate::Expr`, a matchkey), while the field spells the arguments (`Expr<S>`). Those live in
    // the node's own generics, so the check is a generic fn that is never called.
    let subast_are_nodes: Vec<TokenStream> = subast
        .iter()
        .filter_map(|e| {
            let key = e.matchkey().to_string();
            let mut found: Option<TypePath> = None;
            for_each_field_type_of(&input.data, &mut |ty| {
                for_each_type_path(ty, &mut |tp| {
                    if found.is_none()
                        && tp
                            .path
                            .segments
                            .last()
                            .map(|s| s.ident.to_string())
                            .as_deref()
                            == Some(key.as_str())
                    {
                        found = Some(tp.clone());
                    }
                })
            });
            // No matching field: `parse_visiting`'s own warning already covers that case.
            let tp = found?;
            let span = e.path.span();
            Some(quote! {
                const _: () = {
                    fn #{Ident::new("__visiting_subast_entry_is_a_node", span)}
                        #impl_generics () #where_clause
                    {
                        fn assert_node<__VisitingT: ?Sized + #root::visit::__announce::Announce>() {}
                        let _ = assert_node::<#tp>;
                    }
                };
            })
        })
        .collect();

    quote! {
        #(#subast_are_nodes)*
    }
}

/// How to name an item announce handed the check, for the "not a node" error.
fn item_kind(item: &Item) -> &'static str {
    match item {
        Item::Union(_) => "a union has no walkable fields, and",
        Item::Trait(_) => "a trait",
        Item::Type(_) => "a type alias",
        Item::Mod(_) => "a module",
        _ => "this item",
    }
}

/// `#[visiting(..)]` — declare a node: announce it, with the check below.
///
/// A derive cannot announce the item it is on, since it cannot change it, so this is an attribute
/// that writes the `#[announce]`. It takes the follow-list and `crate = ..` as arguments and folds in
/// any further `#[visiting(..)]` on the item: imported as `visiting`, the attribute would otherwise
/// run again on each of them. It reports every relative path at once, where announce would stop at
/// the first. announce and the check are both reached through `#root`, so a node's crate needs no
/// dependency on announce.
pub fn attribute(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut item: Item = match syn::parse2(item) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error(),
    };
    let kind = item_kind(&item);
    let attrs = match &mut item {
        Item::Struct(s) => &mut s.attrs,
        Item::Enum(e) => &mut e.attrs,
        other => abort!(
            other,
            "`#[visiting]` applies to a struct or an enum; {} cannot be an AST node",
            kind
        ),
    };
    let mut attr = match VisitingAttribute::parse.parse2(args) {
        Ok(a) => a,
        Err(e) => {
            emit_error!(e.span(), "invalid `#[visiting(..)]`: {}", e);
            VisitingAttribute::default()
        }
    };
    attr.merge(parse_visiting(attrs));
    attrs.retain(|a| !a.path().is_ident("visiting"));
    let di: DeriveInput = match item.clone() {
        Item::Struct(s) => s.into(),
        Item::Enum(e) => e.into(),
        _ => unreachable!(),
    };
    if !crate::absolute::check(&di) {
        // announce would only repeat the first of the errors reported. The item stays, without the
        // field markers no announcement owns now.
        strip_markers(&mut item);
        return quote!(#item);
    }
    let root = attr
        .krate
        .clone()
        .unwrap_or_else(|| parse_quote!(::visiting));
    let krate = attr.krate.iter();
    let entries = attr.follow.iter().map(|e| {
        let p = &e.path;
        match &e.alias {
            Some(a) => quote!(#p as #a),
            None => quote!(#p),
        }
    });
    quote! {
        #[#root::visit::__announce::announce(
            crate = #root::visit::__announce,
            #root::_imp::visiting( #(crate = #krate,)* #(#entries),* )
        )]
        #item
    }
}

/// Remove the `#[seq]`/`#[opt]`/`#[skip]` field markers, which rustc only accepts on an item whose
/// announcement owns them.
fn strip_markers(item: &mut Item) {
    let marker = |a: &Attribute| ["seq", "opt", "skip"].iter().any(|m| a.path().is_ident(m));
    let fields: Vec<&mut Field> = match item {
        Item::Struct(s) => s.fields.iter_mut().collect(),
        Item::Enum(e) => e
            .variants
            .iter_mut()
            .flat_map(|v| v.fields.iter_mut())
            .collect(),
        _ => Vec::new(),
    };
    for f in fields {
        f.attrs.retain(|a| !marker(a));
    }
}

/// The node declaration as an `announce` definition-side check, which `#[visiting]` hands to the
/// `#[announce]` it writes.
///
/// It asks announce for three things. announce invokes this macro beside the definition, as
/// `visiting! { (ARGS), ITEM }`. It lets the item carry our attributes —
/// `#[visiting(..)]`, `#[seq]`, `#[opt]`, `#[skip]` — which this check *owns*: announce strips them
/// from the item it emits (rustc does not know them) and keeps them in the payload
/// a visitor reads. And it records the check's own arguments in META, so a follow-list may be
/// written there instead.
///
/// Being expanded *beside the definition*, it validates the node where it is written, and asserts
/// that each follow entry is announced too. It emits no metadata of its own: announce's trampoline
/// carries the definition.
pub fn check(input: &announce_core::helper::CheckInput) -> TokenStream {
    // Only a struct or an enum can be a node. `#[announce]` also takes a trait, a type alias and an
    // inline `mod`, so the rule has to be stated here rather than left to the visitor, which would
    // report it as a missing definition.
    let kind = item_kind(&input.item);
    let di: DeriveInput = match input.item.clone() {
        Item::Struct(s) => s.into(),
        Item::Enum(e) => e.into(),
        other => abort!(
            other,
            "`visiting::visiting` applies to a struct or an enum; {} cannot be an AST node",
            kind
        ),
    };
    validate_owned_attributes(input, &di);
    // The follow-list may be written in the check's arguments or in `#[visiting(..)]` on the item,
    // and both at once: one `crate = ..` between them, entries from either.
    let mut attr = match VisitingAttribute::parse.parse2(input.args.clone()) {
        Ok(a) => a,
        Err(e) => {
            emit_error!(e.span(), "invalid `visiting(..)` check: {}", e);
            VisitingAttribute::default()
        }
    };
    attr.merge(parse_visiting(&di.attrs));
    validate_follow(&attr.follow);
    check_ambiguous_last_idents(&di);
    let unmatched_warnings = warn_unmatched_entries(&di, &attr.follow);
    let root = attr.krate.unwrap_or_else(|| parse_quote!(::visiting));
    let announced = follow_announced(&di, &attr.follow, &root);
    quote! { #announced #unmatched_warnings }
}

/// Check where the attributes this check owns were written.
///
/// Misplacement is worth an error rather than silence: an ignored `#[seq]` is a field that is walked
/// the plain way, which no test notices. `#[visiting(..)]` declares the *type*'s follow-list and is
/// read from the item; the three field markers belong to a field, and take no arguments.
fn validate_owned_attributes(input: &announce_core::helper::CheckInput, di: &DeriveInput) {
    fn is_marker(a: &Attribute) -> bool {
        ["seq", "opt", "skip"].iter().any(|n| a.path().is_ident(n))
    }
    fn marker_name(a: &Attribute) -> String {
        a.path().segments.last().unwrap().ident.to_string()
    }
    for a in &di.attrs {
        if is_marker(a) {
            emit_error!(a, "`#[{}]` marks a field, not the type", marker_name(a));
        }
    }
    if let Data::Enum(e) = &di.data {
        for v in &e.variants {
            for a in &v.attrs {
                if is_marker(a) {
                    emit_error!(a, "`#[{}]` marks a field, not a variant", marker_name(a));
                } else if a.path().is_ident("visiting") {
                    emit_error!(
                        a,
                        "`#[visiting(..)]` declares the type's follow-list; write it on the \
                         type (or in the check's arguments), not on a variant"
                    );
                }
            }
        }
    }
    for f in data_field_iter(&di.data) {
        for a in &f.attrs {
            if a.path().is_ident("visiting") {
                emit_error!(
                    a,
                    "`#[visiting(..)]` declares the type's follow-list; write it on the type \
                     (or in the check's arguments), not on a field"
                );
            } else if is_marker(a) && !matches!(a.meta, Meta::Path(_)) {
                emit_error!(a, "`#[{}]` takes no arguments", marker_name(a));
            }
        }
    }
    // Anywhere else syn parses an attribute — a generic parameter, a where-predicate — a marker is
    // neither read nor rejected above, and announce would still strip it. The owned list announce
    // hands back is the whole set, so a count that does not add up means one is somewhere unexpected.
    let accounted = di.attrs.iter().filter(|a| input.owned.owns(a)).count()
        + data_field_iter(&di.data)
            .flat_map(|f| f.attrs.iter())
            .filter(|a| input.owned.owns(a))
            .count()
        + match &di.data {
            Data::Enum(e) => e
                .variants
                .iter()
                .flat_map(|v| v.attrs.iter())
                .filter(|a| input.owned.owns(a))
                .count(),
            _ => 0,
        };
    let found = input.owned_attributes().len();
    if found > accounted {
        emit_error!(
            di.ident,
            "{} of this type's `#[visiting]`/`#[seq]`/`#[opt]`/`#[skip]` attributes {} written \
             somewhere a visitor does not read — not on the type, a variant or a field",
            found - accounted,
            if found - accounted == 1 { "is" } else { "are" }
        );
    }
}
