# ast

Walk a syntax tree you derived.

Put `#[derive(Ast)]` on your nodes, list them in `visitor!`, and you get a visitor over them: one
`visit_*` method per type, each with a body that already descends.

```rust
use ast::Ast;

#[derive(Ast)]
pub enum Expr {
    Lit(u32),
    Neg(Box<Expr>),
    Many(#[seq] Vec<Expr>),
}

pub mod visit {
    ast::visitor!(crate::Expr);
}

fn main() {
    let e = Expr::Many(vec![Expr::Lit(1), Expr::Neg(Box::new(Expr::Lit(2)))]);

    // A closure is the short way in. It runs for every node of the type its argument names.
    let mut n = 0;
    e.visit(|_: &Expr| n += 1);
    assert_eq!(n, 4);
}
```

## Keeping state

A struct lets you carry something along. Override the methods you care about and call the free
function to keep descending — or don't, to prune.

```rust
use ast::Ast;

#[derive(Ast)]
pub enum Expr {
    Lit(u32),
    Neg(Box<Expr>),
}

pub mod visit {
    ast::visitor!(crate::Expr);
}

#[derive(Default)]
struct Depth {
    max: usize,
    at: usize,
}

impl visit::Visit for Depth {
    fn visit_expr(&mut self, i: &Expr) {
        self.at += 1;
        self.max = self.max.max(self.at);
        visit::visit_expr(self, i);
        self.at -= 1;
    }
}
impl visit::VisitMut for Depth {}

fn main() {
    let mut d = Depth::default();
    Expr::Neg(Box::new(Expr::Lit(1))).visit(&mut d);
    assert_eq!(d.max, 2);
}
```

`visit_mut` is the same with `&mut`. A tuple of two to eight *closures* runs them all in a single
walk; a visitor cannot join that tuple, because folding into one hook is what makes the single walk
possible. Two visitors are two walks, written as two calls.

## Editing a list

A `#[seq]` field hands you the slot instead of one element, so you can see an element's index and
its neighbours — and, on the `mut` side, change the list.

```rust
use ast::{Ast, SeqView};

#[derive(Ast)]
pub enum Expr {
    Lit(u32),
    Many(#[seq] Vec<Expr>),
}

pub mod visit {
    ast::visitor!(crate::Expr);
}

struct DropLits;
impl visit::Visit for DropLits {}
impl visit::VisitMut for DropLits {
    fn visit_expr_seq_mut<V: SeqView<Expr>>(&mut self, v: &mut V) {
        v.retain_mut(|e| !matches!(e, Expr::Lit(_)));
    }
}

fn main() {
    let mut e = Expr::Many(vec![Expr::Lit(1), Expr::Many(vec![])]);
    e.visit_mut(&mut DropLits);
    assert!(matches!(&e, Expr::Many(v) if v.len() == 1));
}
```

`#[opt]` does the same for an `Option<T>` slot.

## Reaching further

A field is followed when the visitor knows its type. For a type it does not list — one the walk only
passes *through* — add `#[subast(crate::path::To)]` to the node. `#[skip]` takes a field out of the
walk entirely.

## Extending a visitor

List a type in one `visitor!` only. To cover more, extend the visitor you have.

```rust
use ast::Ast;

#[derive(Ast)]
pub enum Expr {
    Lit(u32),
}

#[derive(Ast)]
#[subast(crate::Expr)]
pub struct Stmt {
    pub e: Expr,
}

pub mod base {
    ast::visitor!(crate::Expr);
}
pub mod ext {
    ast::visitor!(crate::base => crate::Stmt);
}

fn main() {
    let s = Stmt { e: Expr::Lit(1) };

    // A closure reaches an inherited type as readily as one of this visitor's own.
    let mut n = 0;
    s.visit(|_: &Expr| n += 1);
    assert_eq!(n, 1);
}
```

`base::Visit` becomes a supertrait of `ext::Visit`, so a pass written against the base still runs in
the extended walk. A visitor implements every trait in the chain; the ancestors it does not touch
take an empty impl.

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
