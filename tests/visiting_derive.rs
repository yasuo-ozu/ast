//! Stage 2: `#[derive(Visiting)]` emits the marker impl, the metadata callback macro, and the
//! macro-namespace re-export under the type's own name.

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
    // A local callback in announce's shape, which receives the definition and re-parses the embedded
    // definition into a real (renamed) type, proving the metadata macro carries a syn-parseable
    // copy of the original definition and that the path-callback invocation works.
    macro_rules! rebuild {
        ([$($cb:tt)*] $($p:ident)::+ ! => [$($meta:tt)*] { $item:item } @state $_c:path { }) => {
            // The forwarded tokens parse as a single item; re-emit it in a fresh module so it does
            // not clash with the original `Expr`.
            #[allow(dead_code)]
            mod rebuilt {
                use super::*;
                $item
            }
        };
    }

    // `Expr!` resolves to the re-exported metadata macro (macro namespace), distinct from the
    // `Expr` enum (type namespace).
    Expr! { [Expr!, rebuild!] @state rebuild { } }
    let _ = rebuilt::Expr::Other(PhantomData::<()>);
}

#[test]
fn subast_allowlist_round_trips() {
    // The trampoline carries the `#[visiting]` allowlist in announce's META slot, as
    // `follow(<path> as <matchkey>)`. The callback matches a single entry, proving both the format
    // and that the path resolves to a real type.
    macro_rules! check {
        (
            [$($cb:tt)*]
            $($p:ident)::+ ! => [ follow ( $path:path as $key:ident ) ] { $item:item }
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
