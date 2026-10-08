// Every doctest here spells its nodes out at the crate root and then names them in
// `visitor!(crate::Expr)`, so the `fn main` is doing real work: without it rustdoc wraps the whole
// snippet, the nodes become items *inside* `main`, and `crate::Expr` no longer resolves
// (`error[E0433]: could not find Expr in the crate root`). The lint does not model that.
#![allow(clippy::needless_doctest_main)]
#![doc = include_str!("README.md")]
//!
//! # What lives here
//!
//! The generated visitor modules and the `#[derive(Ast)]` metadata macros live in *your* crate;
//! this one holds what they rely on:
//!
//! * [`Ast`] — an empty marker trait implemented by `#[derive(Ast)]` for every AST node type.
//! * [`Repeater`] — the `type-leak` indirection trait. `#[derive(Ast)]` emits one
//!   `impl Repeater<N> for <the AST type>` per field type that depends on the definition's type
//!   context, so a consumer can name those types portably as `<T as ::ast::Repeater<N>>::Type`
//!   regardless of which crate or module it expands in.
//! * [`walk::Walk`]/[`walk::WalkMut`], the [`SeqView`]/[`SeqEdit`]/[`OptView`]/[`OptEdit`] slot views
//!   and [`Elements`], which the
//!   generated descent bottoms out in.

#[doc(hidden)]
pub mod _imp {
    pub use ast_macro;
}

/// The crate's own items, under the name the generated code reaches them by.
///
/// `visitor!` binds this as `__ast_rt` in the module it expands into, so the code it writes names
/// the crate through the macro's `$crate` rather than by name — which is what lets a facade
/// re-export this crate without its users depending on it directly. `use $crate as ..` is not
/// allowed, so the alias needs a module to point at.
#[doc(hidden)]
pub mod visit {
    // Named one by one rather than re-exported with a glob: a glob would also carry `visit` itself
    // (making `ast::visit::visit::visit::…` a valid path) and `_imp`. This is the whole surface the
    // generated code reaches — `#root::visit::{Ast, Repeater}` from the derive, `__ast_rt::{..}`
    // from `visitor!`.
    pub use super::{visitor, walk, Ast, Elements, OptEdit, OptView, Repeater, SeqEdit, SeqView};
}

/// Marks a type as an AST node and emits the metadata [`visitor!`] reads.
///
/// # Types are matched by their last path segment
///
/// **This is the main rule to remember.** A field's type is known by the last part of its path.
/// Nothing else is read. So `other::Node`, `crate::ast::Node` and plain `Node` all mean the same
/// `Node` here. This derive sees one type at a time. It cannot look a path up.
///
/// If two types share a name, give one an alias and write the alias in the field:
///
/// ```ignore
/// #[derive(Ast)]
/// #[subast(crate::other::Node as OtherNode)]
/// pub struct Wrapper {
///     inner: OtherNode,
/// }
/// ```
///
/// # Attributes
///
/// | attribute | on | meaning |
/// |---|---|---|
/// | `#[subast(path, ..)]` | the type | types this node reaches that `visitor!(..)` does **not** list, so the walk can pass through them. A listed type needs no entry. |
/// | `#[seq]` / `#[opt]` | a field | reach the parent slot through a [`SeqView`] or [`OptView`]. Plain `Vec<T>` / `Option<T>` only. |
/// | `#[skip]` | a field | never walk into this field, whatever its type. An error together with `#[seq]` or `#[opt]`. |
///
/// Take a field's type and remove wrappers like `Box`, `Vec` and `Option`. What is left is the
/// **head**. The walk goes into a field when the visitor knows its head. It knows a type if you
/// listed it in `visitor!(..)`, or if it comes from a base visitor. Those need no `#[subast(..)]`
/// entry. That list is only for types the walk passes *through* on the way to a known one.
///
/// A `#[subast(..)]` path must start at a crate: `crate::path::to::Type`, or
/// `other_crate::path::Type` for a type in another crate. A plain name, or a `self::`/`super::`
/// path, is an error. Here is why. This derive puts the path inside a macro. That macro runs later,
/// in the module where the visitor lives. A short path would mean something else there.
/// `visitor!(..)` has no such rule. Only `#[subast(..)]` does.
///
/// A `#[seq]` or `#[opt]` field must be a plain `Vec<T>` or `Option<T>`. A wrapped one, like
/// `Option<Box<T>>`, cannot be edited in place. The macro tells you so, instead of building a view
/// that would not work.
///
/// `#[skip]` takes a field out of the walk, whatever its type:
///
/// ```ignore
/// #[derive(Ast)]
/// pub enum Expr<S> {
///     // `&mut Box<Stmt<'_, S>>` is invariant, so a mut walk can never descend into a concrete
///     // lifetime fill — say so here rather than leaving it to an error inside the macro.
///     Stmt(#[skip] Box<Stmt<'static, S>>),
///     Lit(PhantomData<S>),
/// }
/// ```
pub use ast_macro::Ast;

/// Define a visitor over the given AST types. Put it in its own empty module:
///
/// ```ignore
/// pub mod my_visitor {
///     ast::visitor!(Type, Expr);          // or: visitor!(super::base => Stmt);
/// }
/// ```
///
/// Everything it writes comes twice: once for reading, and once for editing, under a `_mut` name
/// that takes `&mut` where the reading half takes `&`.
///
/// # What it expands to
///
/// Take a `Doc` holding a list of `Item`s:
///
/// ```ignore
/// #[derive(Ast)]
/// pub struct Doc { #[seq] items: Vec<Item> }
///
/// #[derive(Ast)]
/// pub struct Item(u32);
///
/// pub mod visit { ast::visitor!(crate::Doc, crate::Item); }
/// ```
///
/// The module is about a thousand lines. In outline it is this:
///
/// ```ignore
/// // One method per listed type, each defaulted to the walk below. Implement it to act.
/// pub trait Visit {
///     fn visit_doc(&mut self, i: &Doc) { visit_doc(self, i) }
///     fn visit_item(&mut self, i: &Item) { visit_item(self, i) }
///
///     // Only because a field is marked `#[seq]`. It takes the list, not an element — and
///     // `Elements` is in the bound, so the container's own iterator is reachable here.
///     fn visit_item_seq<'a, V: SeqView<Item> + Elements<'a, Item>>(&mut self, v: &'a V)
///         where Item: 'a
///     {
///         for e in v.elements() { self.visit_item(e); }
///     }
/// }
///
/// // The walk for one node, on its own. Call it from an override to keep descending;
/// // leave it out to prune.
/// pub fn visit_doc<V: Visit + ?Sized>(this: &mut V, i: &Doc) {
///     this.visit_item_seq(&i.items);
/// }
///
/// // Empty: a `u32` is not a visited type, so there is nothing inside an `Item` to walk into.
/// pub fn visit_item<V: Visit + ?Sized>(this: &mut V, i: &Item) {}
///
/// // What `node.visit(..)` takes. The conversions behind it are hidden.
/// pub trait IntoVisitor<T> { .. }
///
/// // The entry point, inherent on each listed type — so a walk needs no import, and may start
/// // at any node. It returns `self`, so calls chain.
/// impl Doc {
///     pub fn visit<T>(&self, visitor: impl IntoVisitor<T>) -> &Self {
///         let mut visitor = visitor.into_visitor();
///         visitor.visit_doc(self);
///         self
///     }
///     pub fn visit_mut<T>(&mut self, visitor: impl IntoVisitorMut<T>) -> &mut Self { .. }
/// }
///
/// impl Item {
///     pub fn visit<T>(&self, visitor: impl IntoVisitor<T>) -> &Self { .. }
///     pub fn visit_mut<T>(&mut self, visitor: impl IntoVisitorMut<T>) -> &mut Self { .. }
/// }
///
/// // ...and all of the above again for editing: `VisitMut`, `visit_doc_mut`,
/// // `visit_item_seq_mut`, `IntoVisitorMut`, each `&` become `&mut`. That is how a view edits
/// // the list instead of only reading it.
/// ```
///
/// So `doc.visit(..)` needs nothing in scope. Implementing `Visit` does, and so does calling
/// `into_visitor`.
///
/// That is the whole public surface. The impls behind it — a visitor by `&mut` or in a `Box`, the
/// adapters that make a closure one, the machinery an extending visitor needs — are hidden, named
/// with an `__ast` prefix, and kept where `use path::to::visit::*;` does not reach them.
///
/// # Using it
///
/// ```
/// mod grammar {
///     use ast::Ast;
///
///     #[derive(Ast)]
///     #[subast(crate::grammar::Expr)]
///     pub enum Expr { Lit(u32), Neg(Box<Expr>), Many(#[seq] Vec<Expr>) }
///
///     pub mod visit { ast::visitor!(super::Expr); }
/// }
///
/// fn main() {
///     use grammar::{visit, Expr};
///     use ast::{Elements, SeqEdit};
///
///     // A closure sees one node type.
///     let mut n = 0;
///     Expr::Neg(Box::new(Expr::Lit(1))).visit(|_: &Expr| n += 1);
///     assert_eq!(n, 2);
///
///     // A visitor overrides one method, and calls the free fn to keep descending.
///     struct Depth { max: usize, at: usize }
///     impl visit::Visit for Depth {
///         fn visit_expr(&mut self, i: &Expr) {
///             self.at += 1;
///             self.max = self.max.max(self.at);
///             visit::visit_expr(self, i);
///             self.at -= 1;
///         }
///     }
///     let mut d = Depth { max: 0, at: 0 };
///     Expr::Neg(Box::new(Expr::Lit(1))).visit(&mut d);
///     assert_eq!(d.max, 2);
///
///     // `#[seq]` adds `visit_expr_seq_mut`, which hands over the list, not one element.
///     struct DropLits;
///     impl visit::VisitMut for DropLits {
///         fn visit_expr_seq_mut<'a, V: SeqEdit<Expr> + Elements<'a, Expr>>(
///             &mut self,
///             v: &'a mut V,
///         ) {
///             v.retain_mut(|e| !matches!(e, Expr::Lit(_)));
///         }
///     }
///     let mut e = Expr::Many(vec![Expr::Lit(1), Expr::Neg(Box::new(Expr::Lit(2)))]);
///     e.visit_mut(&mut DropLits);
///     assert!(matches!(&e, Expr::Many(v) if v.len() == 1));
/// }
/// ```
///
/// # `#[seq]` and `#[opt]` fields
///
/// A type held in a field marked `#[seq]` or `#[opt]` gets one more method on each trait:
/// `visit_<node>_seq` and `visit_<node>_opt`, plus their `_mut` halves. They are handed the whole
/// slot rather than one element, and like the others they come with a body that just walks on.
///
/// Reading, you get a `&impl SeqView<T>` or `&impl OptView<T>`; editing, a `&mut impl SeqEdit<T>`
/// or `&mut impl OptEdit<T>`. That shows you what `visit_<node>`
/// cannot: how many elements there are, which index one has, what sits next to it. Editing, you
/// get a `&mut`, so you can `push`, `remove` or `retain_mut` — see the example above.
///
/// A seq view carries a lifetime, because its bound is `SeqView<T> + Elements<'a, T>` and the slot
/// arrives as `&'a V`:
///
/// ```ignore
/// fn visit_item_seq<'a, V: SeqView<Item> + Elements<'a, Item>>(&mut self, v: &'a V)
///     where Item: 'a;
/// ```
///
/// So all three ways of reaching the elements are open inside an override: indexing — `&v[i]`
/// through [`SeqView`]'s `Index` supertrait, `&mut v[i]` through [`SeqEdit`]'s `IndexMut` — and
/// iteration through [`Elements::elements`] / [`Elements::elements_mut`], which hand back the
/// container's **own** iterator. `self` is borrowed separately from the slot, so an override may
/// iterate and call back into the visitor in the same body. An opt view needs no iterator and takes
/// no lifetime: [`OptView::get`] is the element.
///
/// `elements_mut` takes the view's whole `'a` borrow, so it is one pass and the slot is spent after
/// it; an override that wants to walk the elements *and* resize indexes instead (`&mut v[i]`, then
/// `push`/`retain_mut`), which is what the positional half is for.
///
/// When the visited node is itself generic, an override has to **repeat** the `where` clause — rustc
/// checks the impl method's signature for well-formedness on its own and does not carry the trait's
/// predicate over:
///
/// ```ignore
/// fn visit_stmt_seq_mut<'a, V: SeqEdit<Stmt<S>> + Elements<'a, Stmt<S>>>(&mut self, v: &'a mut V)
///     where Stmt<S>: 'a                       // omit this and: error[E0309]: the parameter type
/// { .. }                                      // `S` may not live long enough
/// ```
///
/// A node with no parameters (`Stmt`, not `Stmt<S>`) is `'static`, the predicate is trivially true,
/// and the clause may be left off.
///
/// Resizing is [`SeqEdit`] and filling or emptying is [`OptEdit`], neither of which the shared side
/// asks for — so a fixed-size slot still has a readable view.
///
/// The marker only chooses which method is called. A `Vec<T>` and an `Option<T>` are walked by the
/// same generated code either way, and no container type is ever named.
///
/// # What `visit` takes
///
/// `node.visit(..)` takes anything that implements `IntoVisitor`, which the macro implements for
/// four things:
///
/// * **a visitor** — any type implementing `Visit`, by value or by `&mut`.
/// * **a closure** taking `&T`, for any type the visitor reaches — one it lists, or one it
///   inherits. The argument type picks the node; the closure runs for every `T` in the tree.
/// * **a tuple of 2 to 8 closures**, folded into one hook, so all of them run in a single walk.
/// * **a `Box` around a visitor**, by value.
///
/// ```ignore
/// node.visit(&mut pass);                          // a visitor
/// node.visit(Box::new(pass));                     // boxed
/// node.visit(|e: &Expr| exprs += 1);              // one closure
/// node.visit((|e: &Expr| .., |t: &Type| ..));     // two closures, one walk
/// ```
///
/// A tuple is closures and nothing else. Folding them into one hook is what buys the single walk,
/// and a visitor cannot join that fold because it descends on its own. Two visitors are two walks,
/// written as two calls:
///
/// ```ignore
/// node.visit(&mut first);
/// node.visit(&mut second);
/// ```
///
/// A tuple holding a visitor is rejected:
///
/// ```compile_fail
/// use ast::Ast;
/// #[derive(Ast)]
/// pub enum Expr { Lit(u32) }
/// pub mod visit { ast::visitor!(crate::Expr); }
///
/// struct Pass;
/// impl visit::Visit for Pass {}
/// impl visit::VisitMut for Pass {}
///
/// fn main() {
///     let mut p = Pass;
///     let mut q = Pass;
///     Expr::Lit(1).visit(&mut (p, q));
/// }
/// ```
///
/// `visit_mut` takes the same four things through `IntoVisitorMut`; its closures take `&mut T`.
/// `into_visitor` turns a closure into a standalone visitor value, which then goes wherever a
/// visitor goes.
///
/// # Extending a visitor
///
/// A type may be listed in only one `visitor!` per crate, so to cover more types you add to the
/// visitor you already have. Name the base module, then `=>`, then the new types:
///
/// ```ignore
/// #[derive(Ast)] pub enum Type<S> { .. }
/// #[derive(Ast)] pub enum Expr<S> { Typed(Box<Type<S>>), .. }
/// #[derive(Ast)] pub enum Stmt<S> { E(Box<Expr<S>>), .. }
///
/// pub mod base { ast::visitor!(super::Type, super::Expr); }
/// pub mod ext  { ast::visitor!(super::base => super::Stmt); }
/// ```
///
/// `base::Visit` is a supertrait of `ext::Visit`, so one visitor implements both and has a method
/// for all three types:
///
/// ```ignore
/// impl<S> base::Visit<S> for Counter {
///     fn visit_expr(&mut self, i: &Expr<S>) { self.exprs += 1; base::visit_expr(self, i); }
/// }
/// impl<S> ext::Visit<S> for Counter {
///     fn visit_stmt(&mut self, i: &Stmt<S>) { self.stmts += 1; ext::visit_stmt(self, i); }
/// }
///
/// stmt.visit(&mut counter);   // walks Stmt, then Expr, then Type
/// ```
///
/// A field holding one of the base's types needs no `#[subast(..)]` entry: `ext` asks the base for
/// its list, and those nodes go to the base's `visit_*` through the supertrait. Chains can be as
/// long as you like — `base => mid => new` works, and the new visitor may take more generic
/// parameters than its base. Several bases at once work too: `visitor!(a, b => Stmt)`.
///
/// A closure reaches an inherited type as readily as one of your own, so the short spelling keeps
/// working however deep the chain is:
///
/// ```ignore
/// item.visit(|t: &Type<()>| ..);   // `Type` belongs to `base`, three levels up
/// ```
///
/// One limit: a type from the base gets no `#[seq]`/`#[opt]` view here, because its `visit_*`
/// method lives in the base. Mark that field in the base's own `visitor!` instead, or drop the
/// marker and let the walk take the elements one at a time.
///
/// A visitor has to implement every trait in the chain, including the ancestors whose nodes it
/// does not touch. Those impls are empty — every method already walks — but Rust has no way to ask
/// for them, so a pass that overrides one node of `top` still writes `impl base::Visit for P {}`
/// and its `VisitMut` twin for each ancestor.
///
/// ## Naming an ancestor you cannot reach
///
/// Each module re-exports its base as `__ast_base`, so the ancestor `n` links up is
/// `base_module::__ast_base::..` repeated `n` times. That path goes through the chain rather than
/// to the module itself, so it resolves even when the ancestor is private, or sits in a crate you
/// do not depend on:
///
/// ```ignore
/// impl<S> up::visit_mid::__ast_base::Visit<S> for MyPass { .. }   // `up::visit` is private
/// ```
///
/// # Rules
///
/// Every listed type needs `#[derive(Ast)]`. That derive writes the metadata this macro reads.
///
/// A type is known by the last segment of its path (see [`Ast`]), and the generated names come
/// from that segment, so two listed types ending in the same name are rejected.
///
/// A type may be listed in only one `visitor!` per crate. Each listing gives it its own
/// `visit`/`visit_mut` method, so a second one is `E0592: duplicate definitions`. See
/// [Extending a visitor](#extending-a-visitor).
#[macro_export]
macro_rules! visitor {
    ($($t:tt)*) => {
        // The generated items name this crate through the alias rather than by its own name, so a
        // grammar reaching `visitor!` through a facade needs no direct dependency on it.
        #[doc(hidden)]
        #[allow(unused_imports)]
        use $crate::visit as __ast_rt;
        $crate::_imp::ast_macro::__visitor_entry! { @root { $crate } $($t)* }
    };
}

/// Marker trait implemented by every type carrying `#[derive(Ast)]`.
///
/// It carries no methods; its only purpose is to let generic code (and the `#[visitor]` generator)
/// bound on "is an AST node".
#[cfg_attr(
    has_diagnostic,
    diagnostic::on_unimplemented(
        message = "`{Self}` is not an AST node",
        label = "not an AST node",
        note = "add `#[derive(Ast)]` to `{Self}`; every `#[subast(..)]` entry has to be one"
    )
)]
pub trait Ast {}

/// `type-leak` repeater: passes a single type out of the leaker's type context to a referrer.
///
/// `INDEX` distinguishes the type references collected from one definition (in declaration order,
/// matching `type_leak::Referrer::iter`). The `#[derive(Ast)]` macro implements this directly on
/// the AST type; a consumer refers back through it.
pub trait Repeater<const INDEX: usize> {
    /// The leaked type, valid in the referrer's context via `<T as Repeater<INDEX>>::Type`.
    type Type: ?Sized;
}

// ── Slot views ──────────────────────────────────────────────────────────────────────────────────
//
// A generated `visit_mut` traversal hands a *node held inside another AST in a collection / Option slot*
// a **view of that slot** as an argument, through which the visitor edits the parent **in place** (no
// cloning of existing nodes). The view is a trait implemented directly on the container types — so the
// descent passes `&mut self.field` with no wrapper.
//
// Two of them are about **editing** a slot, and carry no borrow of their own: [`SeqView`] (Vec-like,
// unbounded, positional) and [`OptView`] (Option-like, ≤1), so a view stays usable across a `push`, a
// `remove` and a walk, in any order. The third, [`Elements`], is about **iterating** whatever a
// container holds, and carries its borrow as a trait parameter so each container can name its own
// iterators. They stay apart because the positional half has to work for a slot that is only read, or
// only resized; a generated seq view asks for both at once, over the lifetime of the slot it is handed.
//
// These are **edit** views only: the descent itself goes through [`walk::Walk`]. The element type is a
// **type parameter** (`SeqView<T>`, not an associated type), and the traits are bare-element — a
// `#[seq]`/`#[opt]` field must be a bare `Vec<T>`/`Option<T>`, never a wrapped one.
//
// The type parameter is also what keeps these implementable downstream. A consumer whose nodes are
// local may put any of these on a container it does not own —
// `impl<'a> Elements<'a, MyNode> for BTreeSet<MyNode>` is accepted, because the local `MyNode`
// appears in the trait reference. With the element as an *associated* type the trait reference would
// be `Elements for BTreeSet<MyNode>`, naming nothing local, and the orphan rule would refuse it.

/// Iteration over whatever a container holds: the elements of a sequence, the ≤1 node of an `Option`,
/// the **values** of a map. One trait for every shape, because iteration is the one thing they all
/// agree on — a map has no positional edit, and a sequence has no key.
///
/// The borrow is a parameter of the **trait** rather than of the methods, so the two iterators can be
/// named as associated types: a `Vec` hands back the slice iterators, a map its own `Values`/
/// `ValuesMut`, an association list a `Map<slice::Iter<..>, fn(..) -> ..>`. No `dyn`, no allocation,
/// and no `unsafe` — an iterator synthesized here out of repeated indexing would have to widen each
/// element borrow to the whole walk, which is sound only if every impl yields a distinct element per
/// index, and a safe trait cannot promise that. A GAT would put the lifetime back on the method, and
/// costs 1.65.
///
/// Implement it for your own container and both paths open at once: code holding the container
/// directly calls [`elements`](Self::elements), and so does a `visit_<t>_seq`/`_seq_mut` override,
/// whose view param is bound `SeqView<T> + Elements<'a, T>` for the `'a` of the slot it was handed.
/// A container that is only ever walked or structurally edited can skip it — [`walk::Walk`] and
/// [`SeqEdit`]/[`OptEdit`] need no iterator — but every `SeqView` this crate ships implements it.
pub trait Elements<'a, T: 'a> {
    /// The container's own by-reference iterator.
    type Iter: Iterator<Item = &'a T>;
    /// The container's own by-`&mut` iterator.
    type IterMut: Iterator<Item = &'a mut T>;

    /// Iterate what the container holds, by shared ref. Named `elements` (not `iter`) so it never
    /// shadows the slice `iter` on a concrete `Vec` when the trait is in scope.
    fn elements(&'a self) -> Self::Iter;
    /// Iterate what the container holds, by `&mut`, for in-place edits. Keys, for a map, are untouched.
    fn elements_mut(&'a mut self) -> Self::IterMut;
}

/// A readable, **sequence-like** view of an AST collection field (`Vec`, `VecDeque`, a slice, an
/// array, or a container of your own — implement it and `#[seq]` accepts that container, since no
/// container name is ever matched). Bare-element: the element type is `T` itself, never a wrapped
/// `Box<T>`. A generated `visit_<t>_seq(&self, &impl SeqView<T>)` receives one; its `&mut` half
/// receives a [`SeqEdit`], which is where writing lives.
///
/// Purely positional, and so free of any borrow of its own: an override may read, walk and edit in
/// whatever order it likes. Element access is [`Index`] — `&v[i]`, panicking past the end like every
/// other indexed container — so the trait itself declares only what indexing cannot say: how long
/// the slot is. Iteration is [`Elements`], a separate trait so that a container keeps its own
/// iterator types; a generated seq view asks for both, so `v.elements()` works inside an override.
///
/// Writing is [`SeqEdit`]: `&mut v[i]` through its `IndexMut`, and `insert`/`remove` for the size. A
/// fixed-size slot — a slice, an array — is a `SeqView` and not a `SeqEdit`, which is the whole reason
/// the two are apart: the shared side of a generated visitor asks only for this, so it accepts shapes
/// the `&mut` side cannot.
///
/// [`Index`]: core::ops::Index
pub trait SeqView<T>: core::ops::Index<usize, Output = T> {
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A [`SeqView`] that can also grow and shrink — what a `#[seq]` field's `&mut` side is handed, and
/// what makes `push`/`remove`/`retain_mut` available. Every container that can resize implements both.
pub trait SeqEdit<T>: SeqView<T> + core::ops::IndexMut<usize, Output = T> {
    /// Insert before index `i` (`i == len` appends).
    fn insert(&mut self, i: usize, value: T);
    fn remove(&mut self, i: usize) -> T;

    /// Append. No `Self: Sized`, so it works through `&mut dyn SeqEdit<T>` as well.
    fn push(&mut self, value: T) {
        let n = self.len();
        self.insert(n, value);
    }
    /// Visit each element in place, then drop those for which `f` returns `false`.
    ///
    /// The provided body is **O(n²)**: it can only shrink the slot through `remove`, which every
    /// array-backed container implements by shifting the tail. A container that can do better should
    /// say so — `Vec` and `VecDeque` here forward to their inherent `retain_mut`. The cost is real
    /// and not hypothetical: a generated `visit_<t>_seq_mut` override holds its slot as a generic
    /// `V`, so `v.retain_mut(..)` there is always this method, never an inherent one.
    ///
    /// `Self: Sized` because of the `impl FnMut` argument; the rest of the trait is object-safe.
    fn retain_mut(&mut self, mut f: impl FnMut(&mut T) -> bool)
    where
        Self: Sized,
    {
        let mut i = 0;
        while i < self.len() {
            if f(&mut self[i]) {
                i += 1;
            } else {
                self.remove(i);
            }
        }
    }
}

/// A mutable, **Option-like** view (≤1 element) of an AST `Option` field, bare-element (a nested
/// `Box`-like layer descends separately, through [`walk::Walk`]). A generated
/// `visit_<t>_opt_mut(&mut self, &mut impl OptEdit<T>)` receives one, and `visit_<t>_opt` a `&`.
/// Holding at most one element, it needs no iterator and so — unlike a seq view — takes no
/// lifetime: [`get`](Self::get) is the element.
pub trait OptView<T> {
    fn is_some(&self) -> bool;
    fn get(&self) -> Option<&T>;

    fn is_none(&self) -> bool {
        !self.is_some()
    }
}

/// An [`OptView`] that can also be written — what a `#[opt]` field's `&mut` side is handed. The same
/// read/write line [`SeqView`] and [`SeqEdit`] are drawn on, so the shared side of a generated visitor
/// never asks for the ability to empty a slot it only reads.
pub trait OptEdit<T>: OptView<T> {
    /// Edit the node in place — no clone.
    fn get_mut(&mut self) -> Option<&mut T>;
    /// Fill or replace the node (works on an empty slot).
    fn set(&mut self, value: T);
    fn take(&mut self) -> Option<T>;

    fn clear(&mut self) {
        let _ = self.take();
    }
}

// `SeqView`/`OptView` are **bare-element**: the container holds the viewed node `T` directly (no element
// adapter). A transparent single-slot wrapper (`Box<T>` and user wrappers) is reached by
// [`walk::Walk`] instead. Such a slot can be neither emptied nor filled, so it is descent-only — never a
// `#[seq]`/`#[opt]` edit target.

impl<T> SeqView<T> for Vec<T> {
    fn len(&self) -> usize {
        <[T]>::len(self)
    }
}

impl<T> SeqEdit<T> for Vec<T> {
    // The provided `push`/`retain_mut` would go through `insert`/`remove` one element at a time;
    // `Vec`'s own are amortized O(1) and O(n). Generic code — which is what a visitor override is —
    // reaches these, never the inherent methods.
    fn push(&mut self, value: T) {
        Vec::push(self, value);
    }
    fn retain_mut(&mut self, f: impl FnMut(&mut T) -> bool) {
        Vec::retain_mut(self, f);
    }
    fn insert(&mut self, i: usize, value: T) {
        Vec::insert(self, i, value);
    }
    fn remove(&mut self, i: usize) -> T {
        Vec::remove(self, i)
    }
}

impl<'a, T: 'a> Elements<'a, T> for Vec<T> {
    type Iter = core::slice::Iter<'a, T>;
    type IterMut = core::slice::IterMut<'a, T>;
    fn elements(&'a self) -> Self::Iter {
        <[T]>::iter(self)
    }
    fn elements_mut(&'a mut self) -> Self::IterMut {
        <[T]>::iter_mut(self)
    }
}

impl<T> SeqView<T> for std::collections::VecDeque<T> {
    fn len(&self) -> usize {
        std::collections::VecDeque::len(self)
    }
}

impl<T> SeqEdit<T> for std::collections::VecDeque<T> {
    fn push(&mut self, value: T) {
        std::collections::VecDeque::push_back(self, value);
    }
    fn retain_mut(&mut self, f: impl FnMut(&mut T) -> bool) {
        std::collections::VecDeque::retain_mut(self, f);
    }
    fn insert(&mut self, i: usize, value: T) {
        std::collections::VecDeque::insert(self, i, value);
    }
    fn remove(&mut self, i: usize) -> T {
        std::collections::VecDeque::remove(self, i).expect("index in bounds")
    }
}

// Fixed-size, so `SeqView` and not `SeqEdit`: a slice or an array can be read, indexed and walked,
// but has no `insert`/`remove` to offer. The shared side of a generated visitor takes them.
impl<T> SeqView<T> for [T] {
    fn len(&self) -> usize {
        <[T]>::len(self)
    }
}

impl<T, const N: usize> SeqView<T> for [T; N] {
    fn len(&self) -> usize {
        N
    }
}

// `LinkedList` is a [`Elements`] and a [`walk::Walk`], but not a `SeqView`: `SeqView` reaches an element
// through `Index`, which std does not implement for it (a list has no O(1) position), and the orphan
// rule puts it out of reach here. Iterate it, or walk it.

impl<'a, T: 'a> Elements<'a, T> for std::collections::VecDeque<T> {
    type Iter = std::collections::vec_deque::Iter<'a, T>;
    type IterMut = std::collections::vec_deque::IterMut<'a, T>;
    fn elements(&'a self) -> Self::Iter {
        std::collections::VecDeque::iter(self)
    }
    fn elements_mut(&'a mut self) -> Self::IterMut {
        std::collections::VecDeque::iter_mut(self)
    }
}

impl<T> OptView<T> for Option<T> {
    fn is_some(&self) -> bool {
        Option::is_some(self)
    }
    fn get(&self) -> Option<&T> {
        self.as_ref()
    }
}

impl<T> OptEdit<T> for Option<T> {
    fn get_mut(&mut self) -> Option<&mut T> {
        self.as_mut()
    }
    fn set(&mut self, value: T) {
        *self = Some(value);
    }
    fn take(&mut self) -> Option<T> {
        Option::take(self)
    }
}

impl<'a, T: 'a> Elements<'a, T> for Option<T> {
    type Iter = core::option::Iter<'a, T>;
    type IterMut = core::option::IterMut<'a, T>;
    fn elements(&'a self) -> Self::Iter {
        self.iter()
    }
    fn elements_mut(&'a mut self) -> Self::IterMut {
        self.iter_mut()
    }
}

impl<'a, T: 'a> Elements<'a, T> for std::collections::LinkedList<T> {
    type Iter = std::collections::linked_list::Iter<'a, T>;
    type IterMut = std::collections::linked_list::IterMut<'a, T>;
    fn elements(&'a self) -> Self::Iter {
        std::collections::LinkedList::iter(self)
    }
    fn elements_mut(&'a mut self) -> Self::IterMut {
        std::collections::LinkedList::iter_mut(self)
    }
}

// The two unsized/fixed shapes the walk already handles: a slice and an array. Neither can grow, so
// neither is a `SeqView` — but both iterate, which is all `Elements` asks.
impl<'a, T: 'a> Elements<'a, T> for [T] {
    type Iter = core::slice::Iter<'a, T>;
    type IterMut = core::slice::IterMut<'a, T>;
    fn elements(&'a self) -> Self::Iter {
        self.iter()
    }
    fn elements_mut(&'a mut self) -> Self::IterMut {
        self.iter_mut()
    }
}

impl<'a, T: 'a, const N: usize> Elements<'a, T> for [T; N] {
    type Iter = core::slice::Iter<'a, T>;
    type IterMut = core::slice::IterMut<'a, T>;
    fn elements(&'a self) -> Self::Iter {
        self.iter()
    }
    fn elements_mut(&'a mut self) -> Self::IterMut {
        self.iter_mut()
    }
}

// Not implemented, on purpose: `HashSet`, `BTreeSet` and `BinaryHeap` have no `iter_mut`, because
// handing out a `&mut` to an element would let a visitor break the hash, the ordering or the heap
// invariant that places it. A set-shaped slot is therefore walked (through [`walk::Walk`]) but never
// edited through a view; rebuild the set instead.

// A map holds its node in the VALUE slot, which no positional view can address — so a map is a `Elements`
// and nothing else. The descent reaches it through [`walk::Walk`], whose `Thru<H>` impl for
// `HashMap`/`BTreeMap` walks the values directly; `Elements` is how a hand-written traversal does the same.

impl<'a, K: 'a, V: 'a, S> Elements<'a, V> for std::collections::HashMap<K, V, S> {
    type Iter = std::collections::hash_map::Values<'a, K, V>;
    type IterMut = std::collections::hash_map::ValuesMut<'a, K, V>;
    fn elements(&'a self) -> Self::Iter {
        self.values()
    }
    fn elements_mut(&'a mut self) -> Self::IterMut {
        self.values_mut()
    }
}

impl<'a, K: 'a, V: 'a> Elements<'a, V> for std::collections::BTreeMap<K, V> {
    type Iter = std::collections::btree_map::Values<'a, K, V>;
    type IterMut = std::collections::btree_map::ValuesMut<'a, K, V>;
    fn elements(&'a self) -> Self::Iter {
        self.values()
    }
    fn elements_mut(&'a mut self) -> Self::IterMut {
        self.values_mut()
    }
}

/// The descent: how an AST value hands its parts to a visitor, and the indicators that say which
/// parts those are. The trait `visitor!` generates is the dispatch half; this is the other.
///
/// Every traversal a generated `visit_*` performs is one [`walk::Walk::walk`] call. An indicator —
/// computed per field from the field's *shape* — picks the impl, so the descent never names a leaf
/// type and never matches a container by name. The language is small:
///
/// | indicator | meaning |
/// |---|---|
/// | [`Here`](walk::Here) | `Self` is a visited node — hand it to the visitor |
/// | [`Skip`](walk::Skip) | do not descend |
/// | [`Thru<H>`](walk::Thru) | `Self` is a container — delegate `H` to what it holds |
/// | `(H0, .., Hn)` | `Self` is a tuple — delegate each `Hi` to slot `i` |
///
/// So `Vec<(Length, Line)>` is walked at `Thru<(Skip, Here)>`: through the `Vec`, past the
/// `Length`, into the `Line`.
pub mod walk {
    /// `Self` is a visited node: hand it to the visitor.
    pub struct Here;
    /// Do not descend into `Self`.
    pub struct Skip;
    /// `Self` is a container: delegate `H` to the value(s) it holds.
    pub struct Thru<H>(core::marker::PhantomData<H>);

    /// How an AST value hands itself to a visitor — the descent half of the walk, where the trait
    /// `visitor!` generates is the dispatch half.
    ///
    /// `V` is the visitor. `M` is the tag minted by the `visitor!` module the walk belongs to, so two
    /// visitor modules over the same node do not write conflicting impls. `H` is the indicator:
    /// which parts of `Self` to descend into.
    ///
    /// `H` is what lets the container impls below stay generic in the types they hold — `(Length, Line)`
    /// and `(Line, Line)` are the *same* impl at different indicators — so no leaf type is ever named and
    /// no shape needs an impl of its own.
    #[cfg_attr(
        has_diagnostic,
        diagnostic::on_unimplemented(
            message = "`{Self}` cannot be walked at `{H}`",
            label = "this field's shape has no `Walk` impl",
            note = "a visited node needs `#[derive(Ast)]` and an entry in the owning type's `#[subast(..)]`; \
                a container of one needs a `Walk` impl, which `ast::visit` has for `Vec`, `VecDeque`, \
                `Option`, `Box`, maps, slices, arrays and tuples, and which a crate can add for a \
                container of its own"
        )
    )]
    pub trait Walk<M, H, V: ?Sized> {
        /// Offer the parts of `self` that `H` selects to `v`.
        fn walk(&self, v: &mut V);
    }

    /// The `&mut` half of [`Walk`].
    #[cfg_attr(
        has_diagnostic,
        diagnostic::on_unimplemented(
            message = "`{Self}` cannot be walked mutably at `{H}`",
            label = "this field's shape has no `WalkMut` impl",
            note = "a field behind a shared `&` is walkable on the shared side only"
        )
    )]
    pub trait WalkMut<M, H, V: ?Sized> {
        /// Offer the parts of `self` that `H` selects to `v`, by `&mut`.
        fn walk_mut(&mut self, v: &mut V);
    }

    // `Skip` descends nowhere, whatever it is applied to. A blanket over every type — but at one fixed
    // indicator, so it overlaps nothing else. This is what frees a leaf type from having to say anything
    // about itself, even when it shares a tuple with a node.
    impl<M, V: ?Sized, T: ?Sized> Walk<M, Skip, V> for T {
        fn walk(&self, _: &mut V) {}
    }

    impl<M, V: ?Sized, T: ?Sized> WalkMut<M, Skip, V> for T {
        fn walk_mut(&mut self, _: &mut V) {}
    }

    macro_rules! walk_seq {
        ($($ty:ty $(where $($p:ident),+)?),* $(,)?) => {
            $(
                impl<M, H, V: ?Sized, T: Walk<M, H, V> $(, $($p),+)?> Walk<M, Thru<H>, V> for $ty {
                    fn walk(&self, v: &mut V) {
                        for x in self.iter() {
                            x.walk(v);
                        }
                    }
                }
                impl<M, H, V: ?Sized, T: WalkMut<M, H, V> $(, $($p),+)?> WalkMut<M, Thru<H>, V> for $ty {
                    fn walk_mut(&mut self, v: &mut V) {
                        for x in self.iter_mut() {
                            x.walk_mut(v);
                        }
                    }
                }
            )*
        };
    }

    walk_seq!(
        Vec<T>,
        [T],
        std::collections::VecDeque<T>,
        std::collections::LinkedList<T>
    );

    impl<M, H, V: ?Sized, T: Walk<M, H, V>, const N: usize> Walk<M, Thru<H>, V> for [T; N] {
        fn walk(&self, v: &mut V) {
            for x in self.iter() {
                x.walk(v);
            }
        }
    }

    impl<M, H, V: ?Sized, T: WalkMut<M, H, V>, const N: usize> WalkMut<M, Thru<H>, V> for [T; N] {
        fn walk_mut(&mut self, v: &mut V) {
            for x in self.iter_mut() {
                x.walk_mut(v);
            }
        }
    }

    impl<M, H, V: ?Sized, T: Walk<M, H, V>> Walk<M, Thru<H>, V> for Option<T> {
        fn walk(&self, v: &mut V) {
            if let Some(x) = self {
                x.walk(v);
            }
        }
    }

    impl<M, H, V: ?Sized, T: WalkMut<M, H, V>> WalkMut<M, Thru<H>, V> for Option<T> {
        fn walk_mut(&mut self, v: &mut V) {
            if let Some(x) = self {
                x.walk_mut(v);
            }
        }
    }

    // A map holds the node in its VALUE slot, so only the values are walked.
    macro_rules! walk_map {
        ($($ty:ty),* $(,)?) => {
            $(
                impl<M, H, V: ?Sized, K, T: Walk<M, H, V>> Walk<M, Thru<H>, V> for $ty {
                    fn walk(&self, v: &mut V) {
                        for x in self.values() {
                            x.walk(v);
                        }
                    }
                }
                impl<M, H, V: ?Sized, K, T: WalkMut<M, H, V>> WalkMut<M, Thru<H>, V> for $ty {
                    fn walk_mut(&mut self, v: &mut V) {
                        for x in self.values_mut() {
                            x.walk_mut(v);
                        }
                    }
                }
            )*
        };
    }

    walk_map!(std::collections::HashMap<K, T>, std::collections::BTreeMap<K, T>);

    // A field behind a shared reference is walkable on the shared side only — there is no `&mut` to be had
    // through a `&`, so there is deliberately no `WalkMut` twin.
    impl<M, H, V: ?Sized, T: Walk<M, H, V> + ?Sized> Walk<M, Thru<H>, V> for &T {
        fn walk(&self, v: &mut V) {
            (**self).walk(v);
        }
    }

    /// Transparent single-slot wrappers. A consumer's own wrapper joins the same way:
    ///
    /// ```
    /// # use ast::walk::{Walk, WalkMut, Thru};
    /// struct MyPtr<T>(Box<T>);
    /// impl<M, H, V: ?Sized, T: Walk<M, H, V>> Walk<M, Thru<H>, V> for MyPtr<T> {
    ///     fn walk(&self, v: &mut V) { <T as Walk<M, H, V>>::walk(&self.0, v) }
    /// }
    /// impl<M, H, V: ?Sized, T: WalkMut<M, H, V>> WalkMut<M, Thru<H>, V> for MyPtr<T> {
    ///     fn walk_mut(&mut self, v: &mut V) { <T as WalkMut<M, H, V>>::walk_mut(&mut self.0, v) }
    /// }
    /// ```
    impl<M, H, V: ?Sized, T: Walk<M, H, V> + ?Sized> Walk<M, Thru<H>, V> for Box<T> {
        fn walk(&self, v: &mut V) {
            (**self).walk(v);
        }
    }

    impl<M, H, V: ?Sized, T: WalkMut<M, H, V> + ?Sized> WalkMut<M, Thru<H>, V> for Box<T> {
        fn walk_mut(&mut self, v: &mut V) {
            (**self).walk_mut(v);
        }
    }

    // One impl per arity, fully generic in the slot types: a slot's indicator decides whether it is walked,
    // so `(Length, Line)` and `(Line, Line)` are this same impl at `(Skip, Here)` and `(Here, Here)`.
    macro_rules! walk_tuple {
        ($(($($p:ident . $h:ident . $i:tt),+),)*) => {
            $(
                impl<M, V: ?Sized, $($h,)+ $($p: Walk<M, $h, V>),+> Walk<M, ($($h,)+), V> for ($($p,)+) {
                    fn walk(&self, v: &mut V) {
                        $( self.$i.walk(v); )+
                    }
                }
                impl<M, V: ?Sized, $($h,)+ $($p: WalkMut<M, $h, V>),+> WalkMut<M, ($($h,)+), V> for ($($p,)+) {
                    fn walk_mut(&mut self, v: &mut V) {
                        $( self.$i.walk_mut(v); )+
                    }
                }
            )*
        };
    }

    walk_tuple! {
        (A.HA.0),
        (A.HA.0, B.HB.1),
        (A.HA.0, B.HB.1, C.HC.2),
        (A.HA.0, B.HB.1, C.HC.2, D.HD.3),
        (A.HA.0, B.HB.1, C.HC.2, D.HD.3, E.HE.4),
        (A.HA.0, B.HB.1, C.HC.2, D.HD.3, E.HE.4, F.HF.5),
        (A.HA.0, B.HB.1, C.HC.2, D.HD.3, E.HE.4, F.HF.5, G.HG.6),
        (A.HA.0, B.HB.1, C.HC.2, D.HD.3, E.HE.4, F.HF.5, G.HG.6, I.HI.7),
        (A.HA.0, B.HB.1, C.HC.2, D.HD.3, E.HE.4, F.HF.5, G.HG.6, I.HI.7, J.HJ.8),
        (A.HA.0, B.HB.1, C.HC.2, D.HD.3, E.HE.4, F.HF.5, G.HG.6, I.HI.7, J.HJ.8, K.HK.9),
        (A.HA.0, B.HB.1, C.HC.2, D.HD.3, E.HE.4, F.HF.5, G.HG.6, I.HI.7, J.HJ.8, K.HK.9, L.HL.10),
        (A.HA.0, B.HB.1, C.HC.2, D.HD.3, E.HE.4, F.HF.5, G.HG.6, I.HI.7, J.HJ.8, K.HK.9, L.HL.10, N.HN.11),
    }
}
