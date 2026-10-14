use super::*;

// `#[visitor([base =>] T, U, ...)]` attribute: kicks off the metadata ping-pong.

pub(crate) struct VisitorArgs {
    /// The visitors this one extends — every one of them a supertrait of the generated `Visit`.
    pub(crate) bases: Vec<Path>,
    pub(crate) types: Vec<Path>,
}

impl Parse for VisitorArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.is_empty() {
            return Ok(VisitorArgs {
                bases: Vec::new(),
                types: Vec::new(),
            });
        }
        // One comma-separated list, which a `=>` turns into the bases: `a, b => T, U`. Without it
        // the list was the visited types all along.
        let head: Punctuated<Path, Token![,]> = Punctuated::parse_separated_nonempty(input)?;
        if !input.peek(Token![=>]) {
            return Ok(VisitorArgs {
                bases: Vec::new(),
                types: head.into_iter().collect(),
            });
        }
        input.parse::<Token![=>]>()?;
        let types: Punctuated<Path, Token![,]> = Punctuated::parse_terminated(input)?;
        Ok(VisitorArgs {
            bases: head.into_iter().collect(),
            types: types.into_iter().collect(),
        })
    }
}

pub(crate) fn last_ident(path: &Path) -> &Ident {
    &path.segments.last().unwrap().ident
}

/// Input to `__visitor_entry`: `@root { <path> } [base =>] T, U, ...`.
struct EntryInput {
    root: Path,
    args: VisitorArgs,
}

impl Parse for EntryInput {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<Token![@]>()?;
        let _kw: Ident = input.parse()?; // `root`
        let content;
        braced!(content in input);
        let root: Path = content.parse()?;
        let args: VisitorArgs = input.parse()?;
        Ok(EntryInput { root, args })
    }
}

/// Kick off the metadata ping-pong from a `define_visit!(...)` invocation (function-like, used inside the
/// visitor module). The crate path arrives via `$crate`, captured by the `define_visit!` shim.
pub fn entry(input: TokenStream, nonce: u64) -> TokenStream {
    let EntryInput { root, args } = match syn::parse2(input) {
        Ok(e) => e,
        Err(e) => return e.to_compile_error(),
    };
    if args.types.is_empty() {
        abort!(
            Span::call_site(),
            "define_visit!(..) needs at least one AST type"
        );
    }
    let build: Path = parse_quote!(#root::_imp::visiting_macro::__visitor_build);
    let nonce = nonce.to_string();
    let nonce: TokenStream = nonce.parse().unwrap();
    let all_types = &args.types;

    // `@visited` carries the *full paths* as written, so the generated items name the visited types
    // in the caller's path context. `@fetching` is the path of the type whose def trails the next
    // bounce (so the fetched def is recorded under it).
    let make_state = |fetching: TokenStream, rest: &[Path], bfetch: &[Path], bnow: TokenStream| {
        state_tokens(
            &quote!(),
            bfetch,
            &bnow,
            &build,
            &nonce,
            all_types,
            &[],
            &quote!(),
            &fetching,
            &quote!(),
            rest,
        )
    };

    match args.bases.split_first() {
        // With bases: ask each one for its visited-type list in turn, then fetch all the types.
        // `@bfetch` is what is left to ask and `@bnow` the one being asked, so its reply can be
        // attributed to it. No type is fetched yet, so `@fetching` is empty; the first `build`
        // bounce pops `rest`.
        Some((first, more)) => {
            let state = make_state(quote!(), all_types, more, quote!(#first));
            quote! {
                #first::__visiting_visited ! { @visited #build { #state } }
            }
        }
        // No base: pop the first type now (so `rest` carries the remainder), recording it under
        // `@fetching`.
        None => {
            let first = &args.types[0];
            let state = make_state(quote!(#first), &args.types[1..], &[], quote!());
            // announce's calling convention: the trampoline, then us as the callback. Our state
            // rides in announce's `REST`, which the protocol passes through untouched.
            quote! {
                #first ! { [ #first !, #build ! ] @state #build { #state } }
            }
        }
    }
}

// Subast records carried through the ping-pong.

/// One `<path> as <matchkey>` entry from a type's `#[visiting]`, as carried in the metadata. `key` is
/// the ident a (container-peeled) field head is matched against; `path` is the resolvable path used
/// to fetch that sub-AST's metadata macro and as a drill match-scrutinee.
pub(crate) struct SubEntry {
    pub(crate) path: Path,
    pub(crate) key: Ident,
}

impl Parse for SubEntry {
    fn parse(input: ParseStream) -> Result<Self> {
        let path: Path = input.parse()?;
        input.parse::<Token![as]>()?;
        let key: Ident = input.parse()?;
        Ok(SubEntry { path, key })
    }
}

/// Read a `visiting(..)` META list — a node's follow-list as it rides in an `announce` trampoline.
///
/// The list is a `#[visiting(..)]` argument list, so it is parsed with the attribute's own parser
/// and each entry reduced to its matchkey. Two producers write it: our derive, which has already
/// resolved every entry (`$crate::Expr as Expr`), and a `visiting(..)` check on an `#[announce]`d
/// type, which records whatever the definer wrote — a bare path, or `path as Alias`. A `crate = ..`
/// item names the facade the *defining* crate's impls reach us through and means nothing here, so
/// it is dropped.
pub(crate) fn parse_follow_meta(ts: TokenStream) -> Result<Vec<SubEntry>> {
    let attr = crate::visiting::VisitingAttribute::parse.parse2(ts)?;
    Ok(attr
        .follow
        .into_iter()
        .map(|e| SubEntry {
            key: e.matchkey(),
            path: e.path,
        })
        .collect())
}

/// Read the follow-list a node wrote as `#[visiting(..)]` *inside* the definition.
///
/// A definition fetched from an `announce` trampoline still carries it: the check that declared the
/// node owns the attribute name, so announce strips it from the item it emits but keeps it in the
/// payload — here. Our own derive strips it instead and renders the list into META, so only one of
/// the two ever has entries. Parsing only: the definition was validated where it was written.
pub(crate) fn parse_follow_attrs(item: &Item) -> Result<Vec<SubEntry>> {
    let attrs: &[Attribute] = match item {
        Item::Struct(s) => &s.attrs,
        Item::Enum(e) => &e.attrs,
        Item::Union(u) => &u.attrs,
        _ => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    for a in attrs.iter().filter(|a| a.path().is_ident("visiting")) {
        let tokens = match &a.meta {
            Meta::List(ml) => ml.tokens.clone(),
            _ => continue,
        };
        out.extend(parse_follow_meta(tokens)?);
    }
    Ok(out)
}

pub(crate) fn parse_subentries(ts: TokenStream) -> Result<Vec<SubEntry>> {
    Ok(Punctuated::<SubEntry, Token![,]>::parse_terminated
        .parse2(ts)?
        .into_iter()
        .collect())
}

/// Re-serialize subast entries as `<path> as <key>, ...` for the next ping-pong bounce.
pub(crate) fn subentries_tokens(entries: &[SubEntry]) -> TokenStream {
    let parts: Vec<TokenStream> = entries
        .iter()
        .map(|e| {
            let p = &e.path;
            let k = &e.key;
            quote!(#p as #k)
        })
        .collect();
    quote!( #(#parts),* )
}

/// Whitespace-insensitive string form of a path, for full-path fetch-dedup and drill lookup (so
/// `a::Cast` and `b::Cast` are distinct).
pub(crate) fn norm_path(p: &Path) -> String {
    quote!(#p).to_string().replace(' ', "")
}
