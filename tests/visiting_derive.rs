//! Stage 2: `#[derive(Visiting)]` emits the marker impl and announces the node: announce's trampoline
//! under the type's own name, in the macro namespace, and the `Announce` impl.

use core::marker::PhantomData;
use visiting::Visiting;

#[derive(Visiting)]
#[visiting(crate::Stmt)]
pub enum Expr<S> {
    Stmt(Box<Stmt<S>>),
    Other(PhantomData<S>),
}

#[derive(Visiting)]
#[visiting(crate::Expr)]
pub enum Stmt<S> {
    Expr(Box<Expr<S>>),
    Other(PhantomData<S>),
}

#[derive(Visiting)]
#[visiting(crate::Expr)]
pub struct Wrap<S> {
    pub inner: Expr<S>,
}

#[derive(Visiting)]
pub struct Inner<S>(pub PhantomData<S>);

// `#[visiting(..)]` records this type's sub-AST children + their resolvable paths. The path must be
// fully qualified (`crate`-rooted here), so it resolves wherever the metadata macro is expanded.
#[derive(Visiting)]
#[visiting(crate::Inner)]
pub struct Outer<S> {
    pub child: Inner<S>,
}

/// `Visiting` (the marker trait) is implemented.
fn assert_is_ast<T: Visiting>() {}

#[test]
fn marker_trait_is_implemented() {
    assert_is_ast::<Expr<()>>();
    assert_is_ast::<Stmt<()>>();
    assert_is_ast::<Wrap<()>>();
}

#[test]
fn repeater_is_implemented_on_the_type_itself() {
    // `#[derive(Visiting)]` impls `Repeater<N>` on the AST type directly (no separate leaker host).
    // `Wrap { inner: Expr<S> }` has one context-dependent field, so `Repeater<0>` resolves.
    fn leaked0<T: visiting::Repeater<0>>() -> core::marker::PhantomData<T::Type> {
        core::marker::PhantomData
    }
    let _ = leaked0::<Wrap<()>>();
}

#[test]
fn metadata_macro_round_trips_definition() {
    // A local callback in announce's shape, which receives the definition. The forwarded tokens
    // parse as a single item, and the path-callback invocation works.
    macro_rules! rebuild {
        ([$($cb:tt)*] $($p:ident)::+ ! => [$($meta:tt)*] { $item:item } @state $_c:path { }) => {
            const PAYLOAD: &str = stringify!($item);
        };
    }

    // `Expr!` resolves to the re-exported trampoline (macro namespace), distinct from the `Expr`
    // enum (type namespace).
    Expr! { [Expr!, rebuild!] @state rebuild { } }
    let payload: String = PAYLOAD.split_whitespace().collect();
    // announce takes absolute paths only, so the relative ones ride rooted at `::__visiting_rel`,
    // which a visitor takes off again; what is already absolute is left alone.
    assert!(payload.contains("::__visiting_rel::Box<::__visiting_rel::Stmt<::__visiting_rel::S>>"));
    assert!(payload.contains("::__visiting_rel::PhantomData<::__visiting_rel::S>"));
}

#[test]
fn derive_announces_the_node() {
    // The trampoline is announce's own, so the node is `Announce` like any announced item.
    fn announced<T: announce::Announce>() {}
    announced::<Expr<()>>();
    announced::<Outer<()>>();
}

#[test]
fn subast_allowlist_round_trips() {
    // The trampoline carries the `#[visiting]` allowlist in announce's META slot, as
    // `visiting(<path> as <matchkey>)` — the same name a `visiting(..)` check on an `#[announce]`d
    // type is recorded under, so a visitor reads one spelling whoever wrote the trampoline. The
    // callback matches a single entry, proving both the format and that the path resolves to a real
    // type.
    macro_rules! check {
        (
            [$($cb:tt)*]
            $($p:ident)::+ ! => [ visiting ( $path:path as $key:ident ) ] { $item:item }
            @state $_c:path { }
        ) => {
            use $path as SubastAlias;
            const SUBAST_KEY: &str = stringify!($key);
        };
    }
    Outer! { [Outer!, check!] @state check { } }
    let _: Option<SubastAlias<()>> = None;
    assert_eq!(SUBAST_KEY, "Inner");
}
