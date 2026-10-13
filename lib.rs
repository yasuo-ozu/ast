// Every doctest here spells its nodes out at the crate root and then names them in
// `define_visit!(crate::Expr)`, so the `fn main` is doing real work: without it rustdoc wraps the whole
// snippet, the nodes become items *inside* `main`, and `crate::Expr` no longer resolves
// (`error[E0433]: could not find Expr in the crate root`). The lint does not model that.
#![allow(clippy::needless_doctest_main)]
#![doc = include_str!("README.md")]
//!
//! # What lives here
//!
//! `#[derive(Visiting)]` and `define_visit!` write their code into *your* crate. This crate holds the pieces
//! that code calls into. You rarely name them yourself:
//!
//! * [`Visiting`] — a marker trait, with no methods. `#[derive(Visiting)]` implements it, and that is how
//!   generic code says "is an AST node".
//! * [`SeqView`]/[`SeqEdit`] and [`OptView`]/[`OptEdit`] — the slot views a `#[seq]`/`#[opt]`
//!   method is handed, plus [`Elements`] for iterating one. You *do* name these, in an override.
//! * [`walk::Walk`]/[`walk::WalkMut`] — how a node offers its children to the visitor. Implement
//!   one of these to make a container of your own walkable.
//! * [`Repeater`] — plumbing. It lets the generated code name a field's type from a different
//!   module or crate than the one the node was defined in.
//!
//! # Nodes you did not write
//!
//! A node has to be declared where it is defined, which is a problem for a type in a crate that has
//! never heard of this one. [`announce`] already solves half of it — it publishes a definition's
//! tokens for any crate to read — and this crate's metadata now speaks announce's protocol, so a
//! walk can fetch an announced definition. The other half is the declaration — the [`Visiting`]
//! marker, which only the defining crate may implement, and the attributes a node is written with,
//! which rustc rejects where no derive declares them. [`visiting`](macro@crate::visiting) is that
//! half: a check handed to `#[announce]`, expanded beside the definition, owning the attributes.
//!
//! [`announce`]: https://docs.rs/announce

#[doc(hidden)]
pub mod _imp {
    pub use visiting_macro;
}

/// The crate's own items, under the name the generated code reaches them by.
///
/// `define_visit!` binds this as `__visiting_rt` in the module it expands into, so the code it writes names
/// the crate through the macro's `$crate` rather than by name — which is what lets a facade
/// re-export this crate without its users depending on it directly. `use $crate as ..` is not
/// allowed, so the alias needs a module to point at.
#[doc(hidden)]
pub mod visit {
    // Named one by one rather than re-exported with a glob: a glob would also carry `visit` itself
    // (making `visiting::visit::visit::visit::…` a valid path) and `_imp`. This is the whole surface the
    // generated code reaches — `#root::visit::{Visiting, Repeater}` from the derive, `__visiting_rt::{..}`
    // from `define_visit!`.
    pub use super::{
        define_visit, walk, Elements, OptEdit, OptView, Repeater, SeqEdit, SeqView, Visiting,
    };
}

/// Marks a type as an AST node and emits the metadata [`define_visit!`] reads.
///
/// # Types are matched by their last path segment
///
/// **This is the main rule to remember.** A field's type is known by the last part of its path.
/// Nothing else is read. So `other::Node`, `crate::visiting::Node` and plain `Node` all mean the same
/// `Node` here. This derive sees one type at a time. It cannot look a path up.
///
/// If two types share a name, give one an alias and write the alias in the field:
///
/// ```ignore
/// #[derive(Visiting)]
/// #[visiting(crate::other::Node as OtherNode)]
/// pub struct Wrapper {
///     inner: OtherNode,
/// }
/// ```
///
/// # Attributes
///
/// | attribute | on | meaning |
/// |---|---|---|
/// | `#[visiting(path, ..)]` | the type | types this node reaches that `define_visit!(..)` does **not** list, so the walk can pass through them. A listed type needs no entry. |
/// | `#[seq]` / `#[opt]` | a field | reach the parent slot through a [`SeqView`] or [`OptView`]. Plain `Vec<T>` / `Option<T>` only. |
/// | `#[skip]` | a field | never walk into this field, whatever its type. An error together with `#[seq]` or `#[opt]`. |
///
/// Take a field's type and remove wrappers like `Box`, `Vec` and `Option`. What is left is the
/// **head**. The walk goes into a field when the visitor knows its head. It knows a type if you
/// listed it in `define_visit!(..)`, or if it comes from a base visitor. Those need no `#[visiting(..)]`
/// entry. That list is only for types the walk passes *through* on the way to a known one.
///
/// A `#[visiting(..)]` path must start at a crate: `crate::path::to::Type`, or
/// `other_crate::path::Type` for a type in another crate. A plain name, or a `self::`/`super::`
/// path, is an error. Here is why. This derive puts the path inside a macro. That macro runs later,
/// in the module where the visitor lives. A short path would mean something else there.
/// `define_visit!(..)` has no such rule. Only `#[visiting(..)]` does.
///
/// A `#[seq]` or `#[opt]` field must be a plain `Vec<T>` or `Option<T>`. A wrapped one, like
/// `Option<Box<T>>`, cannot be edited in place. The macro tells you so, instead of building a view
/// that would not work.
///
/// `#[skip]` takes a field out of the walk, whatever its type:
///
/// ```ignore
/// #[derive(Visiting)]
/// pub enum Expr<S> {
///     // A `&mut` walk cannot descend into this field: `&mut Box<Stmt<'static, S>>` is
///     // invariant in the lifetime, so there is no way in. Saying so here gives a clear error
///     // instead of one from inside the macro.
///     Stmt(#[skip] Box<Stmt<'static, S>>),
///     Lit(PhantomData<S>),
/// }
/// ```
pub use visiting_macro::Visiting;

/// Declare a node from an [`announce`] definition, with no derive of ours.
///
/// `announce` solves the first half of the same problem this crate does: it puts a definition's
/// tokens in a `macro_rules!` beside the item so another crate can read them. A type carrying
/// `#[announce]` is therefore already walkable in shape — but being a *node* also means carrying the
/// [`Visiting`] marker, and that impl can only be written in the crate that owns the type. So an
/// announced type had to be marked by hand, and a type from a crate that merely announces could not
/// be marked at all.
///
/// Pass this macro to `#[announce]` as a check:
///
/// ```ignore
/// #[announce::announce(visiting::visiting)]
/// #[visiting(crate::Leaf)]
/// pub struct Doc {
///     #[seq]
///     pub leaves: ::std::vec::Vec<crate::Leaf>,
///     #[skip]
///     pub note: crate::Leaf,
/// }
/// ```
///
/// announce invokes the check beside the definition — where the marker impl is legal — and
/// `define_visit!(crate::Doc, crate::Leaf)` builds a visitor over types that derive nothing of ours.
///
/// # The attributes are the same ones
///
/// `#[visiting(..)]`, `#[seq]`, `#[opt]` and `#[skip]` are not attributes rustc knows; a derive is
/// what normally declares them, and there is no derive here. The check *owns* the four names
/// instead: `#[announce]` asks it which names it owns, removes them from the item it emits, and
/// keeps them in the payload a visitor reads. So a node declares itself exactly as a deriving one
/// does, and the rules are the same ones — see [`Visiting`].
///
/// `#[announce]` has to sit **above** them, since rustc resolves attributes from the top.
///
/// The follow-list may also be written in the check's own arguments,
/// `visiting::visiting(crate::Leaf)`, which is where it ends up in the trampoline's metadata. Use
/// whichever reads better; a node may use both, and `crate = <facade>` goes in either.
///
/// # Two things it does not do
///
/// * It does not emit the `Repeater` table the derive writes for a field type that only resolves in
///   the defining crate. That is announce's own `repeater = ..`, and the two would collide.
/// * It does not go together with `#[derive(Visiting)]` on one type. The derive would never see its
///   attributes, because the check owns them and announce strips them first. Both sides implement
///   the marker, so rustc reports the conflict (E0119) rather than letting it pass.
///
/// [`announce`]: https://docs.rs/announce
pub use visiting_macro::visiting;

/// Define a visitor over the given AST types. Put it in its own empty module:
///
/// ```ignore
/// pub mod my_visitor {
///     visiting::define_visit!(Type, Expr);          // or: define_visit!(super::base => Stmt);
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
/// #[derive(Visiting)]
/// pub struct Doc { #[seq] items: Vec<Item> }
///
/// #[derive(Visiting)]
/// pub struct Item(u32);
///
/// pub mod visit { visiting::define_visit!(crate::Doc, crate::Item); }
/// ```
///
/// The module is about a thousand lines. In outline:
///
/// ```ignore
/// // One method per listed type, each defaulted to the walk below. Implement it to act.
/// pub trait Visit {
///     fn visit_doc(&mut self, i: &Doc) { visit_doc(self, i) }
///     fn visit_item(&mut self, i: &Item) { visit_item(self, i) }
///
///     // Only because a field is marked `#[seq]`. It takes the list, not an element.
///     fn visit_item_seq<'a, V: SeqView<'a, Item>>(&mut self, v: &'a V) where Item: 'a {
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
/// // ...the same inherent pair on every listed type, and all of the above again for editing:
/// // `VisitMut`, `visit_doc_mut`, `visit_item_seq_mut`, `IntoVisitorMut`, each `&` become `&mut`.
/// ```
///
/// So `doc.visit(..)` needs nothing in scope; implementing `Visit` does. Everything else — a visitor
/// by `&mut` or in a `Box`, the adapters that make a closure one, the machinery an extending visitor
/// needs — is hidden behind an `__ast` prefix and out of reach of a glob import.
///
/// # Using it
///
/// ```
/// mod grammar {
///     use visiting::Visiting;
///
///     #[derive(Visiting)]
///     #[visiting(crate::grammar::Expr)]
///     pub enum Expr { Lit(u32), Neg(Box<Expr>), Many(#[seq] Vec<Expr>) }
///
///     pub mod visit { visiting::define_visit!(super::Expr); }
/// }
///
/// fn main() {
///     use grammar::{visit, Expr};
///     use visiting::SeqEdit;
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
///         fn visit_expr_seq_mut<'a, V: SeqEdit<'a, Expr>>(&mut self, v: &'a mut V) {
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
/// The view shows you what `visit_<node>` cannot: how many elements there are, which index one has,
/// what sits next to it. The `&mut` side can `push`, `remove` or `retain_mut` — see the example
/// above. Each method takes the lifetime of the slot it is handed:
///
/// ```ignore
/// fn visit_item_seq<'a, V: SeqView<'a, Item>>(&mut self, v: &'a V) where Item: 'a;
/// fn visit_item_seq_mut<'a, V: SeqEdit<'a, Item>>(&mut self, v: &'a mut V) where Item: 'a;
/// ```
///
/// One bound reaches everything: index with `&v[i]` or `&mut v[i]`, iterate with
/// [`elements`](Elements::elements) or [`elements_mut`](Elements::elements_mut). `self` is borrowed
/// separately from the slot, so an override may iterate and call back into the visitor in one body.
/// An opt view reaches its ≤1 node by [`OptView::get`].
///
/// If the visited node is itself generic, **repeat the `where` clause** — rustc will not carry the
/// trait's predicate over to your impl:
///
/// ```ignore
/// fn visit_stmt_seq_mut<'a, V: SeqEdit<'a, Stmt<S>>>(&mut self, v: &'a mut V)
///     where Stmt<S>: 'a                       // omit this and: error[E0309]: the parameter type
/// { .. }                                      // `S` may not live long enough
/// ```
///
/// A node with no parameters is `'static`, so the clause may be left off.
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
/// A tuple is closures and nothing else: folding them into one hook is what buys the single walk,
/// and a visitor descends on its own, so it cannot join. Two visitors are two walks:
///
/// ```ignore
/// node.visit(&mut first);
/// node.visit(&mut second);
/// ```
///
/// A tuple holding a visitor is rejected:
///
/// ```compile_fail
/// use visiting::Visiting;
/// #[derive(Visiting)]
/// pub enum Expr { Lit(u32) }
/// pub mod visit { visiting::define_visit!(crate::Expr); }
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
/// A type may be listed in only one `define_visit!` per crate, so to cover more types you add to the
/// visitor you already have. Name the base module, then `=>`, then the new types:
///
/// ```ignore
/// #[derive(Visiting)] pub enum Type<S> { .. }
/// #[derive(Visiting)] pub enum Expr<S> { Typed(Box<Type<S>>), .. }
/// #[derive(Visiting)] pub enum Stmt<S> { E(Box<Expr<S>>), .. }
///
/// pub mod base { visiting::define_visit!(super::Type, super::Expr); }
/// pub mod ext  { visiting::define_visit!(super::base => super::Stmt); }
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
/// A few things follow from that:
///
/// * A field holding one of the base's types needs no `#[visiting(..)]` entry. `ext` asks the base
///   for its list, and those nodes reach the base's `visit_*` through the supertrait.
/// * Chains can be as long as you like: `base => mid => new`.
/// * The new visitor may take more generic parameters than its base.
/// * Several bases at once work: `define_visit!(a, b => Stmt)`.
///
/// A closure reaches an inherited type as readily as one of your own, so the short spelling keeps
/// working however deep the chain is:
///
/// ```ignore
/// item.visit(|t: &Type<()>| ..);   // `Type` belongs to `base`, three levels up
/// ```
///
/// One limit: a type from the base gets no `#[seq]`/`#[opt]` view here, because its `visit_*`
/// method lives in the base. Mark that field in the base's own `define_visit!` instead, or drop the
/// marker and let the walk take the elements one at a time.
///
/// You have to implement **every** trait in the chain, even the ancestors whose nodes you never
/// touch. Those impls are empty, since every method already walks on its own:
///
/// ```ignore
/// impl base::Visit for P {}       // nothing to say, but still required
/// impl base::VisitMut for P {}
/// impl ext::Visit for P { /* the one method you care about */ }
/// impl ext::VisitMut for P {}
/// ```
///
/// ## Naming an ancestor you cannot reach
///
/// Each module re-exports its base as `__visiting_base`, so the ancestor `n` links up is
/// `base_module::__visiting_base::..` repeated `n` times. That path goes through the chain rather than
/// to the module itself, so it resolves even when the ancestor is private, or sits in a crate you
/// do not depend on:
///
/// ```ignore
/// impl<S> up::visit_mid::__visiting_base::Visit<S> for MyPass { .. }   // `up::visit` is private
/// ```
///
/// # Rules
///
/// Every listed type needs `#[derive(Visiting)]`. That derive writes the metadata this macro reads.
///
/// A type is known by the last segment of its path (see [`Visiting`]), and the generated names come
/// from that segment, so two listed types ending in the same name are rejected.
///
/// A type may be listed in only one `define_visit!` per crate. Each listing gives it its own
/// `visit`/`visit_mut` method, so a second one is `E0592: duplicate definitions`. See
/// [Extending a visitor](#extending-a-visitor).
#[macro_export]
macro_rules! define_visit {
    ($($t:tt)*) => {
        // The generated items name this crate through the alias rather than by its own name, so a
        // grammar reaching `define_visit!` through a facade needs no direct dependency on it.
        #[doc(hidden)]
        #[allow(unused_imports)]
        use $crate::visit as __visiting_rt;
        $crate::_imp::visiting_macro::__visitor_entry! { @root { $crate } $($t)* }
    };
}

/// Marker trait implemented by every type declared a node — by `#[derive(Visiting)]`, or by a
/// [`visiting(..)`](macro@crate::visiting) check handed to `#[announce]`.
///
/// It carries no methods; its only purpose is to let generic code (and the `#[visitor]` generator)
/// bound on "is an AST node".
#[cfg_attr(
    has_diagnostic,
    diagnostic::on_unimplemented(
        message = "`{Self}` is not an AST node",
        label = "not an AST node",
        note = "declare `{Self}` a node — `#[derive(Visiting)]`, or a `visiting(..)` check in its \
            `#[announce]` — since every entry in a follow-list has to be one"
    )
)]
pub trait Visiting {}

/// Plumbing, so generated code can name a type it cannot write a path to.
///
/// A node's field type may only be nameable where the node was defined — behind a private module,
/// or through an alias. `#[derive(Visiting)]` records each such type as an impl of this trait on the
/// node, numbered in declaration order, and the generated visitor writes
/// `<Node as Repeater<0>>::Type` instead of a path. You should not need to write this yourself.
///
/// Built on the `type-leak` crate; `INDEX` matches `type_leak::Referrer::iter` order.
pub trait Repeater<const INDEX: usize> {
    /// The recorded type, nameable from anywhere as `<Node as Repeater<INDEX>>::Type`.
    type Type: ?Sized;
}

// ── Slot views ──────────────────────────────────────────────────────────────────────────────────
//
// A generated `visit_mut` traversal hands a *node held inside another AST in a collection / Option slot*
// a **view of that slot** as an argument, through which the visitor edits the parent **in place** (no
// cloning of existing nodes). The view is a trait implemented directly on the container types — so the
// descent passes `&mut self.field` with no wrapper.
//
// [`Elements`] is the root — iteration, both ways. On top of it sit the four views: [`SeqView`]
// (Vec-like, positional) and [`SeqEdit`], [`OptView`] (Option-like, ≤1) and [`OptEdit`]. So one
// bound on a generated method reaches reading, iterating and editing at once.
//
// These are **edit** views only; the descent itself goes through [`walk::Walk`]. They are
// bare-element — a `#[seq]`/`#[opt]` field must be a bare `Vec<T>`/`Option<T>`, never a wrapped one.
//
// The element is a type parameter rather than an associated type, which is what lets a consumer
// implement these for a container it does not own: `Elements<'a, MyNode> for BTreeSet<MyNode>` names
// a local type and so clears the orphan rule.

/// Iterate whatever a container holds: the elements of a sequence, the ≤1 node of an `Option`, the
/// **values** of a map. One trait for every shape — a map has no index, a sequence has no key, but
/// both iterate.
///
/// The borrow is on the trait, not the method, so each container names its own iterator as an
/// associated type: a `Vec` hands back the slice iterator, a map its `Values`. No `dyn`, no
/// allocation, no `unsafe`.
///
/// Every slot view has this as a supertrait, so anything a generated visitor hands you can be
/// iterated.
///
/// ```
/// use visiting::Elements;
/// use std::collections::BTreeMap;
///
/// let v = vec![1, 2, 3];
/// assert_eq!(v.elements().sum::<i32>(), 6);
///
/// let mut v = vec![1, 2, 3];
/// for x in v.elements_mut() {
///     *x *= 10;
/// }
/// assert_eq!(v, vec![10, 20, 30]);
///
/// // A map iterates its values; the keys are not touched.
/// let m = BTreeMap::from([("a", 1), ("b", 2)]);
/// assert_eq!(m.elements().sum::<i32>(), 3);
/// ```
pub trait Elements<'a, T: 'a> {
    /// The container's own by-reference iterator.
    type Iter: Iterator<Item = &'a T>;
    /// The container's own by-`&mut` iterator.
    type IterMut: Iterator<Item = &'a mut T>;

    /// Iterate what the container holds, by shared reference.
    ///
    /// Called `elements` rather than `iter` so that having this trait in scope never changes what
    /// `vec.iter()` means.
    fn elements(&'a self) -> Self::Iter;
    /// Iterate what the container holds, by `&mut`, for in-place edits. Keys, for a map, are untouched.
    fn elements_mut(&'a mut self) -> Self::IterMut;
}

/// A readable, **sequence-like** view of a `#[seq]` slot: `Vec`, `VecDeque`, a slice, an array, or a
/// container of your own — implement this and `#[seq]` accepts it, since no container name is ever
/// matched. A generated `visit_<t>_seq` receives one.
///
/// Read an element with `&v[i]` (the [`Index`] supertrait, panicking past the end), count them with
/// [`len`](Self::len), or iterate through [`Elements`]. Writing is [`SeqEdit`]; a fixed-size slot
/// implements only this one, which is why the two are separate.
///
/// [`Index`]: core::ops::Index
/// ```
/// use visiting::SeqView;
///
/// fn second<'a, V: SeqView<'a, i32>>(v: &'a V) -> Option<&'a i32> {
///     if v.len() < 2 {
///         return None;
///     }
///     Some(&v[1])
/// }
///
/// assert_eq!(second(&vec![1, 2, 3]), Some(&2));
/// assert_eq!(second(&vec![1]), None);
/// ```
pub trait SeqView<'a, T: 'a>: core::ops::Index<usize, Output = T> + Elements<'a, T> {
    /// How many elements the slot holds.
    fn len(&self) -> usize;

    /// Whether the slot holds none.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A [`SeqView`] that can also grow and shrink — what a `#[seq]` field's `&mut` side is handed.
///
/// ```
/// use visiting::SeqEdit;
///
/// // The shape a `visit_<t>_seq_mut` override takes: generic over the slot, so it works for any
/// // container the field might be.
/// fn keep_even<'a, V: SeqEdit<'a, i32>>(v: &'a mut V) {
///     v.retain_mut(|x| *x % 2 == 0);
///     v.push(10);
/// }
///
/// let mut v = vec![1, 2, 3, 4];
/// keep_even(&mut v);
/// assert_eq!(v, vec![2, 4, 10]);
/// ```
pub trait SeqEdit<'a, T: 'a>: SeqView<'a, T> + core::ops::IndexMut<usize, Output = T> {
    /// Insert before index `i`. `i == len` appends.
    ///
    /// # Panics
    ///
    /// If `i > len`.
    fn insert(&mut self, i: usize, value: T);

    /// Take the element at `i` out, shifting the rest down.
    ///
    /// # Panics
    ///
    /// If `i >= len`.
    fn remove(&mut self, i: usize) -> T;

    /// Append.
    fn push(&mut self, value: T) {
        let n = self.len();
        self.insert(n, value);
    }
    /// Visit each element in place, then drop those for which `f` returns `false`.
    ///
    /// Override it if your container can do better than the provided body, which shrinks through
    /// `remove` and so is O(n²) on anything array-backed. `Vec` and `VecDeque` forward to their
    /// inherent `retain_mut`. Generic code — a visitor override — always reaches this method, never
    /// an inherent one, so the cost is not hypothetical.
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

/// A readable, **Option-like** view (≤1 element) of a `#[opt]` slot. A generated `visit_<t>_opt`
/// receives one; [`get`](Self::get) is the element, since indexing would mean nothing here. Writing
/// is [`OptEdit`].
pub trait OptView<'a, T: 'a>: Elements<'a, T> {
    /// Whether the slot is filled.
    fn is_some(&self) -> bool;
    /// The node, if there is one.
    fn get(&self) -> Option<&T>;

    /// Whether the slot is empty.
    fn is_none(&self) -> bool {
        !self.is_some()
    }
}

/// An [`OptView`] that can also be written — what a `#[opt]` field's `&mut` side is handed.
///
/// ```
/// use visiting::OptEdit;
///
/// fn default_to_zero<'a, V: OptEdit<'a, i32>>(v: &'a mut V) {
///     if v.is_none() {
///         v.set(0);
///     }
/// }
///
/// let mut slot: Option<i32> = None;
/// default_to_zero(&mut slot);
/// assert_eq!(slot, Some(0));
/// ```
pub trait OptEdit<'a, T: 'a>: OptView<'a, T> {
    /// Edit the node in place, without replacing it.
    fn get_mut(&mut self) -> Option<&mut T>;
    /// Fill the slot, or replace what is in it.
    fn set(&mut self, value: T);
    /// Empty the slot and hand back what was in it.
    fn take(&mut self) -> Option<T>;

    /// Empty the slot, dropping what was in it.
    fn clear(&mut self) {
        let _ = self.take();
    }
}

// `SeqView`/`OptView` are **bare-element**: the container holds the viewed node `T` directly (no element
// adapter). A transparent single-slot wrapper (`Box<T>` and user wrappers) is reached by
// [`walk::Walk`] instead. Such a slot can be neither emptied nor filled, so it is descent-only — never a
// `#[seq]`/`#[opt]` edit target.

impl<'a, T: 'a> SeqView<'a, T> for Vec<T> {
    fn len(&self) -> usize {
        <[T]>::len(self)
    }
}

impl<'a, T: 'a> SeqEdit<'a, T> for Vec<T> {
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

impl<'a, T: 'a> SeqView<'a, T> for std::collections::VecDeque<T> {
    fn len(&self) -> usize {
        std::collections::VecDeque::len(self)
    }
}

impl<'a, T: 'a> SeqEdit<'a, T> for std::collections::VecDeque<T> {
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
impl<'a, T: 'a> SeqView<'a, T> for [T] {
    fn len(&self) -> usize {
        <[T]>::len(self)
    }
}

impl<'a, T: 'a, const N: usize> SeqView<'a, T> for [T; N] {
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

impl<'a, T: 'a> OptView<'a, T> for Option<T> {
    fn is_some(&self) -> bool {
        Option::is_some(self)
    }
    fn get(&self) -> Option<&T> {
        self.as_ref()
    }
}

impl<'a, T: 'a> OptEdit<'a, T> for Option<T> {
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
/// parts those are. The trait `define_visit!` generates is the dispatch half; this is the other.
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

    /// How a value offers its children to a visitor. Implement it to make a container of your own
    /// walkable; `ast` already covers `Vec`, `VecDeque`, `LinkedList`, `Option`, `Box`, maps,
    /// slices, arrays and tuples.
    ///
    /// Three parameters:
    ///
    /// * `V` — the visitor.
    /// * `H` — the indicator, which says *which* parts of `Self` to descend into. See the table on
    ///   the [module](self) docs.
    /// * `M` — a tag belonging to the `define_visit!` module this walk is for, so two visitors over the
    ///   same node do not collide.
    ///
    /// Because the shape lives in `H` rather than in the impl, one impl covers every container of a
    /// given form: `(Length, Line)` and `(Line, Line)` are the same impl at different indicators.
    ///
    /// A container of your own needs one impl per side. Stay generic in `M`, `H` and `V` and pass
    /// the indicator straight through — then your type works in any visitor, holding any node:
    ///
    /// ```
    /// use visiting::walk::{Thru, Walk, WalkMut};
    ///
    /// pub struct Stack<T>(Vec<T>);
    ///
    /// // `Thru<H>` means "this is a container": hand `H` on to each thing it holds.
    /// impl<M, H, V: ?Sized, T: Walk<M, H, V>> Walk<M, Thru<H>, V> for Stack<T> {
    ///     fn walk(&self, v: &mut V) {
    ///         for x in &self.0 {
    ///             x.walk(v);
    ///         }
    ///     }
    /// }
    ///
    /// impl<M, H, V: ?Sized, T: WalkMut<M, H, V>> WalkMut<M, Thru<H>, V> for Stack<T> {
    ///     fn walk_mut(&mut self, v: &mut V) {
    ///         for x in &mut self.0 {
    ///             x.walk_mut(v);
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// That is all a `#[seq]`-less field needs. To make it an edit target too, add
    /// [`SeqView`](crate::SeqView)/[`SeqEdit`](crate::SeqEdit) — or
    /// [`OptView`](crate::OptView)/[`OptEdit`](crate::OptEdit) for a one-slot shape — and
    /// [`Elements`](crate::Elements) to iterate it.
    #[cfg_attr(
        has_diagnostic,
        diagnostic::on_unimplemented(
            message = "`{Self}` cannot be walked at `{H}`",
            label = "this field's shape has no `Walk` impl",
            note = "a visited node needs `#[derive(Visiting)]` and an entry in the owning type's `#[visiting(..)]`; \
                a container of one needs a `Walk` impl, which `visiting::visit` has for `Vec`, `VecDeque`, \
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
    /// # use visiting::walk::{Walk, WalkMut, Thru};
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
