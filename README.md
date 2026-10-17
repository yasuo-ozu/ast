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

Walk a syntax tree you declared.

Declare your nodes with `#[visiting]`, list them in `define_visit!`, and you get a visitor over
them: one `visit_*` method per type, each with a body that already descends. One example
carries the whole API — a closure, a visitor with state, editing a list, and extending a visitor:

```rust
use visiting::SeqEdit;

// ── the nodes ───────────────────────────────────────────────────────────────────────────────────
// A node is announced with [`announce`](https://docs.rs/announce): its definition is read again
// wherever a visitor is built, so every path in it is absolute.
#[visiting::visiting]
pub enum Expr {
    Lit(u32),
    Neg(::std::boxed::Box<crate::Expr>),
    // `#[seq]` asks for the whole slot, not one element.
    Many(#[seq] ::std::vec::Vec<crate::Expr>),
}

// `#[visiting]` names a type this visitor should follow but does not list; `#[skip]` takes a field
// out of the walk entirely.
#[visiting::visiting]
#[visiting(crate::Expr)]
pub struct Stmt {
    pub e: crate::Expr,
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

## Nodes are announced

A visitor is built from its nodes' definitions, wherever they live — another module, or another
crate. [`announce`](https://docs.rs/announce) publishes a definition's tokens for any crate to read,
so `#[visiting]` announces the node: it writes the item's `#[announce]`, with a check that owns the
field markers. The crate that defines a node needs no dependency on announce. Write `#[visiting]`
first among the item's attributes.

The follow-list goes in the attribute's arguments, or in further `#[visiting(..)]` attributes below
it:

```rust,ignore
#[visiting::visiting(crate::Leaf)]
pub struct Doc {
    #[seq]
    pub leaves: ::std::vec::Vec<crate::Leaf>,
    #[skip]
    pub note: crate::Leaf,
}
```

Field types are `crate::`-rooted and std's absolute, and bounds absolute, because announce re-uses
the payload where it is read. That is announce's rule; `#[visiting]` reports every path that breaks
it, with the absolute form of a std name.

A node that another reader needs announced as well — a `#[coinduction]` module exports a type by
announcing it — writes that `#[announce]` too. The announcements of an item merge into one
trampoline, so both must come from the same version of announce:

```rust,ignore
#[visiting::visiting]
#[announce::announce]
pub struct Boxed<T>(pub ::core::option::Option<::std::boxed::Box<T>>);
```

## Using it through another crate

The code `#[visiting]` emits names `::visiting`. If you reach this crate through a facade that
re-exports it, name the attribute through the facade and tell it so:

```rust,ignore
#[my_crate::visiting(crate = ::my_crate)]
pub enum Expr { Lit(u32) }
```

`define_visit!` needs no such hint — it finds the crate on its own.

## License

MIT.
