# ast

Walk a syntax tree you derived.

Put `#[derive(Ast)]` on your nodes, list them in `visitor!`, and you get a visitor over them: one
`visit_*` method per type, each with a body that already descends.

```rust
use ast::visit::Ast;

#[derive(Ast)]
pub enum Expr {
    Lit(u32),
    Neg(Box<Expr>),
    Many(#[seq] Vec<Expr>),
}

pub mod visit {
    ast::visit::visitor!(crate::Expr);
}
```

## Visiting

A closure is the short way in. It runs for every node of the type its argument names:

```rust
let mut n = 0;
expr.visit(|_: &Expr| n += 1);
```

A struct lets you keep state. Override the methods you care about, and call the free function to
keep descending — or don't, to prune:

```rust
struct Depth { max: usize, at: usize }

impl visit::Visit for Depth {
    fn visit_expr(&mut self, i: &Expr) {
        self.at += 1;
        self.max = self.max.max(self.at);
        visit::visit_expr(self, i);
        self.at -= 1;
    }
}
```

`visit_mut` is the same with `&mut`.

## Editing a list

A `#[seq]` field hands you the slot instead of one element, so you can see an element's index and
its neighbours — and, on the `mut` side, change the list:

```rust
impl visit::VisitMut for DropLits {
    fn visit_expr_seq_mut<V: SeqView<Expr>>(&mut self, v: &mut V) {
        v.retain_mut(|e| !matches!(e, Expr::Lit(_)));
    }
}
```

`#[opt]` does the same for an `Option<T>` slot.

## Reaching further

A field is followed when the visitor knows its type. For a type it does not list — one the walk only
passes *through* — add `#[subast(crate::path::To)]` to the node. `#[skip]` takes a field out of the
walk entirely.

## Extending a visitor

List a type in one `visitor!` only. To cover more, extend the visitor you have:

```rust
pub mod base { ast::visit::visitor!(crate::Expr); }
pub mod ext  { ast::visit::visitor!(crate::base => crate::Stmt); }
```

`base::Visit` becomes a supertrait of `ext::Visit`, so a pass written against the base still runs in
the extended walk.

## Using it through another crate

The code a node emits names `::ast`. If you reach this crate through a facade that re-exports it,
name that instead:

```rust
#[derive(Ast)]
#[ast(::my_crate)]
pub enum Expr { .. }
```

`visitor!` needs no such hint — it finds the crate on its own.

## License

MIT.
