<img src="https://raw.githubusercontent.com/yasuo-ozu/visiting/main/logo.png" alt="visiting" width="420">

# visiting crate [![Latest Version]][crates.io] [![Documentation]][docs.rs] [![CI]][ci-workflow] [![MSRV]][msrv-workflow]

[Latest Version]: https://img.shields.io/crates/v/visiting.svg
[crates.io]: https://crates.io/crates/visiting
[Documentation]: https://img.shields.io/docsrs/visiting
[docs.rs]: https://docs.rs/visiting/latest/visiting/
[CI]: https://github.com/yasuo-ozu/visiting/actions/workflows/ci.yml/badge.svg
[ci-workflow]: https://github.com/yasuo-ozu/visiting/actions/workflows/ci.yml
[MSRV]: https://img.shields.io/crates/msrv/visiting.svg
[msrv-workflow]: https://github.com/yasuo-ozu/visiting/actions/workflows/msrv.yml

Walk a syntax tree you derived.

Put `#[derive(Visiting)]` on your nodes, list them in `define_visit!`, and you get a visitor over them: one
`visit_*` method per type, each with a body that already descends. One example carries the whole
API — a closure, a visitor with state, editing a list, and extending a visitor:

```rust
use visiting::{Visiting, SeqEdit};

// ── the nodes ───────────────────────────────────────────────────────────────────────────────────
#[derive(Visiting)]
pub enum Expr {
    Lit(u32),
    Neg(Box<Expr>),
    // `#[seq]` asks for the whole slot, not one element.
    Many(#[seq] Vec<Expr>),
}

// `#[visiting]` names a type this visitor should follow but does not list; `#[skip]` takes a field
// out of the walk entirely.
#[derive(Visiting)]
#[visiting(crate::Expr)]
pub struct Stmt {
    pub e: Expr,
}

// ── the visitors ────────────────────────────────────────────────────────────────────────────────
// List a type in one `define_visit!` only. To cover more, extend the visitor you have: `base::Visit`
// becomes a supertrait of `ext::Visit`, so a pass written against the base still runs here.
pub mod base {
    visiting::define_visit!(crate::Expr);
}
pub mod ext {
    visiting::define_visit!(crate::base => crate::Stmt);
}

// ── a visitor carrying state ────────────────────────────────────────────────────────────────────
// Override the methods you care about, and call the free function to keep descending — or leave it
// out to prune.
#[derive(Default)]
struct Depth {
    max: usize,
    at: usize,
}

impl base::Visit for Depth {
    fn visit_expr(&mut self, i: &Expr) {
        self.at += 1;
        self.max = self.max.max(self.at);
        base::visit_expr(self, i);
        self.at -= 1;
    }
}
impl base::VisitMut for Depth {}

// ── a visitor editing a list ────────────────────────────────────────────────────────────────────
// A `#[seq]` field adds `visit_expr_seq`/`visit_expr_seq_mut`. They hand over the slot, so you can
// see an element's index and its neighbours — and, on the `mut` side, change the list.
struct DropLits;
impl base::Visit for DropLits {}
impl base::VisitMut for DropLits {
    fn visit_expr_seq_mut<'a, V: SeqEdit<'a, Expr>>(&mut self, v: &'a mut V) {
        v.retain_mut(|e| !matches!(e, Expr::Lit(_)));
    }
}

fn main() {
    let e = Expr::Many(vec![Expr::Lit(1), Expr::Neg(Box::new(Expr::Lit(2)))]);

    // A closure is the short way in: it runs for every node of the type its argument names.
    let mut n = 0;
    e.visit(|_: &Expr| n += 1);
    assert_eq!(n, 4);

    // A struct carries something along.
    let mut d = Depth::default();
    e.visit(&mut d);
    assert_eq!(d.max, 3);

    // `visit_mut` is the same with `&mut`, and the view edits the parent in place.
    let mut m = Expr::Many(vec![Expr::Lit(1), Expr::Many(vec![])]);
    m.visit_mut(&mut DropLits);
    assert!(matches!(&m, Expr::Many(v) if v.len() == 1));

    // The extended visitor reaches an inherited type as readily as one of its own.
    let s = Stmt { e: Expr::Lit(1) };
    let mut k = 0;
    s.visit(|_: &Expr| k += 1);
    assert_eq!(k, 1);

    // A tuple of two to eight closures runs them all in a *single* walk, each on the node type
    // its own argument names — here one of `ext`'s types and one of `base`'s.
    let mut stmts = 0;
    let mut exprs = 0;
    s.visit((|_: &Stmt| stmts += 1, |_: &Expr| exprs += 1));
    assert_eq!((stmts, exprs), (1, 1));
}
```

`#[opt]` does for an `Option<T>` slot what `#[seq]` does for a list.

A tuple takes closures and nothing else. Folding them into one hook is what buys the single walk,
and a visitor descends on its own, so it cannot join — two visitors are two walks and two calls.

A visitor implements every trait in its chain; the ancestors it does not touch take an empty impl,
as `impl base::VisitMut for Depth {}` does above.

## Using it through another crate

The code a node emits names `::visiting`. If you reach this crate through a facade that re-exports it,
name that instead:

```rust,ignore
#[derive(Visiting)]
#[visiting(crate = ::my_crate)]
pub enum Expr { Lit(u32) }
```

`define_visit!` needs no such hint — it finds the crate on its own.

## Nodes from a crate that only announces

A node has to be declared where it is defined, so a type in a crate that never heard of this one
cannot be walked. [`announce`](https://docs.rs/announce) publishes a definition's tokens for any
crate to read, and this crate's metadata speaks announce's protocol — so the definition is reachable.
What is left is the `Visiting` marker, which only the defining crate may implement. Hand this crate's
check to `#[announce]` and it is written there:

```rust,ignore
#[announce::announce(visiting::visiting)]
#[visiting(crate::Leaf)]
pub struct Doc {
    #[seq]
    pub leaves: ::std::vec::Vec<crate::Leaf>,
    #[skip]
    pub note: crate::Leaf,
}
```

`#[derive(Visiting)]` is not involved, and the attributes are the same ones. Rustc knows none of
them — a derive is what normally declares them — so the check *owns* the four names: `#[announce]`
removes them from the item it emits and keeps them in the payload, which is where
`define_visit!(crate::Doc, crate::Leaf)` reads them. `#[announce]` has to sit above them, since
rustc resolves attributes from the top. The follow-list can also go in the check's own arguments,
`visiting::visiting(crate::Leaf)`.

Field types are `crate::`-rooted and bounds absolute because announce re-uses the payload in the
consuming crate; that is announce's own rule. A type that derives `Visiting` must not also carry the
check — the derive would never see its attributes — and rustc says so: both sides implement the
marker, which is a conflicting-implementations error.

## Derived nodes are announced

`#[derive(Visiting)]` announces the node itself, through announce: the type implements
`announce::Announce`, and its trampoline is announce's. announce accepts absolute paths only, so the
derive roots the other paths of the payload at `::__visiting_rel::..` and `define_visit!` takes the
root off; any other reader sees paths that resolve nowhere.

A node that another reader needs announced as well — a `#[coinduction]` module exports a type by
announcing it — writes `#[announce]` above the derive, and keeps one trampoline:

```rust,ignore
#[announce::announce]
#[derive(Visiting)]
#[visiting(crate::Leaf)]
pub struct Boxed<T>(pub ::core::option::Option<::std::boxed::Box<T>>);
```

The derive leaves the trampoline to that announcement, which carries the follow-list and the field
markers in its payload, and checks the item's paths as for any announced item. A
`#[derive(Announce)]` works the same if it is an attribute of its own below the derive and not next
to it: the derive cannot see one in its own list, and rustfmt merges adjacent derives into one. This
needs the merging of announcements in announce's `merge-announcements` branch.

## License

MIT.
