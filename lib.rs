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
//! * [`walk::Walk`]/[`walk::WalkMut`] and the [`SeqView`]/[`OptView`]/[`MapView`] slot views, which the
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
    pub use super::*;
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
///     // Only because a field is marked `#[seq]`. It takes the list, not an element.
///     fn visit_item_seq<V: SeqView<Item>>(&mut self, v: &V) {
///         for e in v.view_iter() { self.visit_item(e); }
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
///     use ast::SeqView;
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
///         fn visit_expr_seq_mut<V: SeqView<Expr>>(&mut self, v: &mut V) {
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
/// Reading, you get a `&impl SeqView<T>` or `&impl OptView<T>`. That shows you what `visit_<node>`
/// cannot: how many elements there are, which index one has, what sits next to it. Editing, you
/// get a `&mut`, so you can `push`, `remove` or `retain_mut` — see the example above.
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

// ── Structural-edit views ───────────────────────────────────────────────────────────────────────
//
// A generated `visit_mut` traversal hands a *node held inside another AST in a collection / Option slot*
// a **view of that slot** as an argument, through which the visitor edits the parent **in place** (no
// cloning of existing nodes). The view is a trait implemented directly on the container types — so the
// descent passes `&mut self.field` with no wrapper. Two are edit targets: [`SeqView`] (Vec-like,
// unbounded) and [`OptView`] (Option-like, ≤1). Two are descent-only: [`MapView`], because a map holds
// the node in its VALUE slot, which no positional view can address.
//
// These are **edit** views only: the descent itself goes through [`walk::Walk`]. The element type is a
// **type parameter** (`SeqView<T>`, not an associated type), and the traits are bare-element — a
// `#[seq]`/`#[opt]` field must be a bare `Vec<T>`/`Option<T>`, never a wrapped one.

/// A mutable, **sequence-like** view of an AST collection field (`Vec`/`VecDeque`/`Punctuated`),
/// bare-element — the element type is `T` itself, never a wrapped `Box<T>`. A generated
/// `visit_<t>_seq_mut(&mut self, &mut impl SeqView<T>)` receives one (and `visit_<t>_seq` a `&`); override it to edit the collection in
/// place. The required core (`len`/`get`/`get_mut`/`insert`/`remove`) is object-safe; the ergonomic
/// helpers are `Self: Sized` provided methods.
pub trait SeqView<T> {
    fn len(&self) -> usize;
    fn get(&self, i: usize) -> Option<&T>;
    /// Edit an element in place — no clone.
    fn get_mut(&mut self, i: usize) -> Option<&mut T>;
    /// Insert before index `i` (`i == len` appends).
    fn insert(&mut self, i: usize, value: T);
    fn remove(&mut self, i: usize) -> T;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn push(&mut self, value: T)
    where
        Self: Sized,
    {
        let n = self.len();
        self.insert(n, value);
    }
    /// Visit each element in place, then drop those for which `f` returns `false`.
    fn retain_mut(&mut self, mut f: impl FnMut(&mut T) -> bool)
    where
        Self: Sized,
    {
        let mut i = 0;
        while i < self.len() {
            let keep = match self.get_mut(i) {
                Some(e) => f(e),
                None => true,
            };
            if keep {
                i += 1;
            } else {
                self.remove(i);
            }
        }
    }
    /// Iterate the elements by shared ref (`for x in v.view_iter() { … }`). Default impl over `get`.
    /// Named `view_iter` (not `iter`) so it never shadows the slice `iter` on a concrete `Vec` when
    /// `SeqView` is in scope.
    fn view_iter(&self) -> SeqIter<'_, T, Self>
    where
        Self: Sized,
    {
        SeqIter {
            seq: self,
            idx: 0,
            len: self.len(),
            _element: core::marker::PhantomData,
        }
    }
    /// Iterate the elements by `&mut` for in-place edits (`for x in v.view_iter_mut() { … }`). For
    /// structural changes use `push`/`insert`/`remove`/`retain_mut`. Default impl over the by-index
    /// `get_mut`. Named `view_iter_mut` to avoid shadowing the slice `iter_mut`.
    fn view_iter_mut(&mut self) -> SeqIterMut<'_, T, Self>
    where
        Self: Sized,
    {
        let len = self.len();
        SeqIterMut {
            seq: self,
            idx: 0,
            len,
            _element: core::marker::PhantomData,
        }
    }
}

/// The shared iterator returned by [`SeqView::view_iter`] — yields each element by index.
pub struct SeqIter<'a, T, V: SeqView<T>> {
    seq: &'a V,
    idx: usize,
    len: usize,
    // `T` is named only by the bound, which does not count as a use.
    _element: core::marker::PhantomData<&'a T>,
}

impl<'a, T, V: SeqView<T>> Iterator for SeqIter<'a, T, V> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        if self.idx >= self.len {
            return None;
        }
        let i = self.idx;
        self.idx += 1;
        // `self.seq` is a `&'a` (Copy) borrow, so `get` returns `&'a T` — no lifetime widening needed
        // (shared borrows may coexist).
        self.seq.get(i)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.len - self.idx;
        (n, Some(n))
    }
}

impl<'a, T, V: SeqView<T>> ExactSizeIterator for SeqIter<'a, T, V> {}

/// The `&mut` iterator returned by [`SeqView::view_iter_mut`] — yields each element once, by index.
pub struct SeqIterMut<'a, T, V: SeqView<T>> {
    seq: &'a mut V,
    idx: usize,
    len: usize,
    _element: core::marker::PhantomData<&'a mut T>,
}

impl<'a, T, V: SeqView<T>> Iterator for SeqIterMut<'a, T, V> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<&'a mut T> {
        if self.idx >= self.len {
            return None;
        }
        let i = self.idx;
        self.idx += 1;
        // SAFETY: each index is yielded exactly once, so the returned `&mut T`s are pairwise disjoint, and
        // all borrow from `self.seq` (a `&'a mut` collection that outlives them) — so widening the element
        // borrow to `'a` is sound.
        self.seq.get_mut(i).map(|r| unsafe { &mut *(r as *mut T) })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.len - self.idx;
        (n, Some(n))
    }
}

impl<'a, T, V: SeqView<T>> ExactSizeIterator for SeqIterMut<'a, T, V> {}

/// A **map-like** view of an AST map field (`HashMap`/`BTreeMap`): the node sits in the VALUE slot, so
/// the viewed element is the map's value type rather than its first type argument. Descent-only — a map
/// slot is keyed, so there is no positional structural edit and hence no `#[seq]`/`#[opt]` counterpart.
///
/// **No longer part of the walk.** A map field descends through [`walk::Walk`], whose `Thru<H>` impl for
/// `HashMap`/`BTreeMap` iterates the values directly. This trait is kept as a way to write that
/// traversal by hand; nothing `visitor!` generates refers to it.
/// The borrow is a parameter of the trait rather than of the methods, so the two iterators can be
/// named as associated types: a map yields its own `Values`/`ValuesMut`, with no `dyn` and no
/// allocation. A GAT would put the lifetime back on the method, and costs 1.65.
pub trait MapView<'a, T: 'a> {
    /// The map's own by-reference value iterator.
    type Iter: Iterator<Item = &'a T>;
    /// The map's own by-`&mut` value iterator.
    type IterMut: Iterator<Item = &'a mut T>;

    fn view_iter(&'a self) -> Self::Iter;
    /// Iterate the values by `&mut` for in-place edits (keys are untouched).
    fn view_iter_mut(&'a mut self) -> Self::IterMut;
}

impl<'a, K: 'a, V: 'a, S> MapView<'a, V> for std::collections::HashMap<K, V, S> {
    type Iter = std::collections::hash_map::Values<'a, K, V>;
    type IterMut = std::collections::hash_map::ValuesMut<'a, K, V>;
    fn view_iter(&'a self) -> Self::Iter {
        self.values()
    }
    fn view_iter_mut(&'a mut self) -> Self::IterMut {
        self.values_mut()
    }
}

impl<'a, K: 'a, V: 'a> MapView<'a, V> for std::collections::BTreeMap<K, V> {
    type Iter = std::collections::btree_map::Values<'a, K, V>;
    type IterMut = std::collections::btree_map::ValuesMut<'a, K, V>;
    fn view_iter(&'a self) -> Self::Iter {
        self.values()
    }
    fn view_iter_mut(&'a mut self) -> Self::IterMut {
        self.values_mut()
    }
}

/// A mutable, **Option-like** view (≤1 element) of an AST `Option` field, bare-element (a nested
/// `Box`/`Attempt` layer descends separately, through [`walk::Walk`]). A generated
/// `visit_<t>_opt_mut(&mut self, &mut impl OptView<T>)` receives one, and `visit_<t>_opt` a `&`.
pub trait OptView<T> {
    fn is_some(&self) -> bool;
    fn get(&self) -> Option<&T>;
    /// Edit the node in place — no clone.
    fn get_mut(&mut self) -> Option<&mut T>;
    /// Fill or replace the node (works on an empty slot).
    fn set(&mut self, value: T);
    fn take(&mut self) -> Option<T>;

    fn is_none(&self) -> bool {
        !self.is_some()
    }
    fn clear(&mut self) {
        let _ = self.take();
    }
    /// Iterate the node by shared ref — 0 or 1 items. Named `view_iter` (not `iter`) to mirror
    /// [`SeqView::view_iter`] and stay non-shadowing.
    fn view_iter(&self) -> core::option::IntoIter<&T> {
        self.get().into_iter()
    }
    /// Iterate the node by `&mut` — 0 or 1 items (in-place edit). Mirrors [`SeqView::view_iter_mut`].
    fn view_iter_mut(&mut self) -> core::option::IntoIter<&mut T> {
        self.get_mut().into_iter()
    }
}

// `SeqView`/`OptView` are **bare-element**: the container holds the viewed node `T` directly (no element
// adapter). A transparent single-slot wrapper (`Box<T>`/`Attempt<T>`/user wrappers) is reached by
// [`walk::Walk`] instead. Such a slot can be neither emptied nor filled, so it is descent-only — never a
// `#[seq]`/`#[opt]` edit target.

impl<T> SeqView<T> for Vec<T> {
    fn len(&self) -> usize {
        <[T]>::len(self)
    }
    fn get(&self, i: usize) -> Option<&T> {
        <[T]>::get(self, i)
    }
    fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        <[T]>::get_mut(self, i)
    }
    fn insert(&mut self, i: usize, value: T) {
        Vec::insert(self, i, value);
    }
    fn remove(&mut self, i: usize) -> T {
        Vec::remove(self, i)
    }
}

impl<T> SeqView<T> for std::collections::VecDeque<T> {
    fn len(&self) -> usize {
        std::collections::VecDeque::len(self)
    }
    fn get(&self, i: usize) -> Option<&T> {
        std::collections::VecDeque::get(self, i)
    }
    fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        std::collections::VecDeque::get_mut(self, i)
    }
    fn insert(&mut self, i: usize, value: T) {
        std::collections::VecDeque::insert(self, i, value);
    }
    fn remove(&mut self, i: usize) -> T {
        std::collections::VecDeque::remove(self, i).expect("index in bounds")
    }
}

impl<T> OptView<T> for Option<T> {
    fn is_some(&self) -> bool {
        Option::is_some(self)
    }
    fn get(&self) -> Option<&T> {
        self.as_ref()
    }
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

    walk_seq!(Vec<T>, [T], std::collections::VecDeque<T>);

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
