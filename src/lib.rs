//! Walk a derived syntax tree.
//!
//! [`macro@visit::Ast`] marks a node and records what it holds; [`visit::visitor!`] turns a set of
//! nodes into a visitor over them. See [`visit`] for the whole surface.

pub mod visit;

#[doc(hidden)]
pub mod _imp {
    pub use ast_macro;
}
