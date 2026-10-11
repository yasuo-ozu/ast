# ast crate [![Latest Version]][crates.io] [![Documentation]][docs.rs] [![CI]][ci-workflow] [![MSRV]][msrv-workflow]

[Latest Version]: https://img.shields.io/crates/v/ast.svg
[crates.io]: https://crates.io/crates/ast
[Documentation]: https://img.shields.io/docsrs/ast
[docs.rs]: https://docs.rs/ast/latest/ast/
[CI]: https://github.com/yasuo-ozu/ast/actions/workflows/ci.yml/badge.svg
[ci-workflow]: https://github.com/yasuo-ozu/ast/actions/workflows/ci.yml
[MSRV]: https://img.shields.io/crates/msrv/ast.svg
[msrv-workflow]: https://github.com/yasuo-ozu/ast/actions/workflows/msrv.yml

Walk a syntax tree you derived.

Put `#[derive(Ast)]` on your nodes, list them in `visitor!`, and you get a visitor over them: one
`visit_*` method per type, each with a body that already descends. One example carries the whole
API — a closure, a visitor with state, editing a list, and extending a visitor:

```rust
use ast::{Ast, SeqEdit};

// ── the nodes ───────────────────────────────────────────────────────────────────────────────────
#[derive(Ast)]
pub enum Expr {
    Lit(u32),
    Neg(Box<Expr>),
    // `#[seq]` asks for the whole slot, not one element.
    Many(#[seq] Vec<Expr>),
}

// `#[subast]` names a type this visitor should follow but does not list; `#[skip]` takes a field
// out of the walk entirely.
#[derive(Ast)]
#[subast(crate::Expr)]
pub struct Stmt {
    pub e: Expr,
}

// ── the visitors ────────────────────────────────────────────────────────────────────────────────
// List a type in one `visitor!` only. To cover more, extend the visitor you have: `base::Visit`
// becomes a supertrait of `ext::Visit`, so a pass written against the base still runs here.
pub mod base {
    ast::visitor!(crate::Expr);
}
pub mod ext {
    ast::visitor!(crate::base => crate::Stmt);
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

The code a node emits names `::ast`. If you reach this crate through a facade that re-exports it,
name that instead:

```rust,ignore
#[derive(Ast)]
#[ast(::my_crate)]
pub enum Expr { Lit(u32) }
```

`visitor!` needs no such hint — it finds the crate on its own.

## License

MIT.
