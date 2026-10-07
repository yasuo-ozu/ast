//! Decide whether `#[diagnostic::on_unimplemented]` can be used.
//!
//! The attribute makes a missing `Walk` impl readable, but its namespace is only stable from 1.78.
//! Gating it on the compiler in hand keeps that message where it is available without making it the
//! crate's minimum.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // `rustc-check-cfg` is itself only understood from 1.80; older cargo ignores the unknown key.
    if version_check::is_min_version("1.80").unwrap_or(false) {
        println!("cargo:rustc-check-cfg=cfg(has_diagnostic)");
    }
    if version_check::is_min_version("1.78").unwrap_or(false) {
        println!("cargo:rustc-cfg=has_diagnostic");
    }
}
