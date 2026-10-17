//! Build-time diagnostics for visitor footguns (clear errors instead of opaque cascades). See the
//! `tests/ui/*.rs` companions.

mod ui_toolchain;

#[test]
fn diagnostics() {
    if !ui_toolchain::should_run("diagnostics") {
        return;
    }
    let t = trybuild::TestCases::new();
    // Two visited types sharing a last segment collide on generated names.
    t.compile_fail("tests/ui/visited_collision.rs");
    // Drilling that would loop through a cycle of unlisted intermediates is a compile error ("list one").
    t.compile_fail("tests/ui/drill_cycle.rs");
    // The container-edit views (`visit_*_seq`/`visit_*_opt`) are generated ONLY for a field marked
    // `#[seq]`/`#[opt]` (no auto-detection); overriding them for an unmarked field is a "not a member of
    // trait" error.
    t.compile_fail("tests/ui/edit_unmarked_no_view.rs");
    // Two different paths ending in the same ident inside one definition: the head match is by last
    // segment, so they cannot be told apart.
    t.compile_fail("tests/ui/ambiguous_last_ident.rs");
    // The same clash, but spread along an inheritance chain — one type per visitor, never meeting in
    // a single node or a single `define_visit!(..)` list.
    t.compile_fail("tests/ui/chain_last_ident_collision.rs");
    // A `#[seq]`/`#[opt]` field can't view an inherited (non-targeted) type — clean error, not E0599.
    t.compile_fail("tests/ui/edit_seq_inherited.rs");
    // `use <visitor module>::*;` carries the traits and the free fns, never the chain machinery.
    t.compile_fail("tests/ui/glob_import_hides_chain.rs");
    // The same type listed twice — one clean error, not an `E0428` cascade on generated names.
    t.compile_fail("tests/ui/visited_listed_twice.rs");
    // A `#[visiting]` entry that is not itself a node — caught at the derive, by the field's type.
    t.compile_fail("tests/ui/subast_entry_not_a_node.rs");
    // `#[derive(Visiting)]` on a union.
    t.compile_fail("tests/ui/derive_visiting_on_a_union.rs");
    // A derived node's relative paths, all reported at once.
    t.compile_fail("tests/ui/derive_relative_path.rs");
    // `impl_chain!` given a method no ancestor declares.
    t.compile_fail("tests/ui/impl_chain_unknown_method.rs");
    // Marker on a non-viewable / container-less / non-visited field → clean abort, not a cryptic trait error.
    t.compile_fail("tests/ui/edit_marker_array.rs");
    t.compile_fail("tests/ui/edit_marker_noncontainer.rs");
    t.compile_fail("tests/ui/edit_marker_unvisited.rs");
    // A `#[seq]`/`#[opt]` field whose top-level type wraps a container (`Box<Vec<T>>`) is not an edit
    // target — edit views need a bare single container (the field still descends).
    t.compile_fail("tests/ui/edit_marker_boxed.rs");
    // The `visiting(..)` check: it marks a node, so only a struct or an enum may carry it; its
    // follow-list obeys the attribute's rooted-path rule; it does not go with the derive; and the
    // attributes it owns have one place each.
    t.compile_fail("tests/ui/visiting_check_on_a_trait.rs");
    t.compile_fail("tests/ui/visiting_check_unrooted_path.rs");
    t.compile_fail("tests/ui/visiting_check_and_attribute.rs");
    t.compile_fail("tests/ui/visiting_check_entry_not_a_node.rs");
    t.compile_fail("tests/ui/visiting_check_misplaced_attrs.rs");
    // Handing a visited type to a generic parameter the node holds but never follows (`Brackets<T, S>`
    // with `items: Vec<T>`) puts those nodes out of reach — previously a silent empty walk.
    t.compile_fail("tests/ui/generic_element.rs");
    // A follow-list entry matching no field is inert, so it is reported as a warning; the fixture
    // denies it to pin the text.
    t.compile_fail("tests/ui/visiting_unmatched_entry.rs");
}
