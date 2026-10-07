//! Two visited types sharing a last segment (`a::Foo`, `b::Foo`) would collide on every generated
//! name (`visit_foo`, `FooHook`, inherent `visit`, …). `__visitor_build` rejects this with a clear
//! message instead of a cascade of duplicate-definition errors.

use core::marker::PhantomData;

mod a {
    use core::marker::PhantomData;
    use ast::Ast;
    #[derive(Ast)]
    pub enum Foo<S> {
        A(PhantomData<S>),
    }
}

mod b {
    use core::marker::PhantomData;
    use ast::Ast;
    #[derive(Ast)]
    pub enum Foo<S> {
        B(PhantomData<S>),
    }
}

pub mod vis {
    ast::visitor!(crate::a::Foo, crate::b::Foo);
}

fn main() {
    let _ = PhantomData::<()>;
}
