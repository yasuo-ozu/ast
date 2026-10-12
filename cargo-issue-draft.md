# `cargo publish --dry-run` cannot detect a reserved crate name

### Problem

crates.io keeps a list of reserved crate names and refuses to publish them. Cargo does not know
about this list. So `--dry-run` says everything is fine, and the real publish fails.

Today the reserved crate names only exists in database migrations:
[`migrations/20170305095748_create_reserved_crate_names/up.sql`](https://github.com/rust-lang/crates.io/blob/main/migrations/20170305095748_create_reserved_crate_names/up.sql)

```sql
INSERT INTO reserved_crate_names (name) VALUES
    ('alloc'), ('arena'), ('ast'), ('builtins'), ('collections'),
    ('compiler-builtins'), ('compiler-rt'), ('compiletest'), ('core'), ('coretest'),
    ('debug'), ('driver'), ('flate'), ('fmt_macros'), ('grammar'), ('graphviz'),
    ('macro'), ('macros'), ('proc_macro'), ('rbml'), ('rust-installer'), ('rustbook'),
    ('rustc'), ('rustc_back'), ('rustc_borrowck'), ('rustc_driver'), ('rustc_llvm'),
    ('rustc_resolve'), ('rustc_trans'), ('rustc_typeck'), ('rustdoc'), ('rustllvm'),
    ('rustuv'), ('serialize'), ('std'), ('syntax'), ('test'), ('unicode');
```

### Steps

```console
$ cargo new --lib ast && cd ast
# add description / license / repository so the publish is otherwise valid

$ cargo publish --dry-run
   Uploading ast v0.1.0 (/tmp/ast)
warning: aborting upload due to dry run
$ echo $?
0

$ cargo publish
error: failed to publish to registry at https://crates.io

Caused by:
  the remote server responded with an error (status 400 Bad Request): cannot upload a crate with a reserved name
```

### Why `--dry-run` misses it

As the SQL above shows, the check is a database trigger. Nothing answers it except an actual
`INSERT`, which happens only when a publish request arrives.

Cargo returns before the only request that would reach it, in
[`cargo_publish.rs`](https://github.com/rust-lang/cargo/blob/master/src/ops/registry/cargo_publish.rs):

```rust
if dry_run {
    gctx.shell().warn("aborting upload due to dry run")?;
    return Ok(());
}
let warnings = registry.publish(&new_crate, tarball)...
```

Cargo's own name check in
[`restricted_names.rs`](https://github.com/rust-lang/cargo/blob/master/crates/cargo-util-schemas/src/restricted_names.rs)
only looks at the spelling: empty, leading digit, bad character. It has no list of denied names.

### Possible solutions

**1. Add a validation endpoint and call it from `--dry-run`.**

[crates.io#1515](https://github.com/rust-lang/crates.io/issues/1515) asked for this in 2018. It is
closed, but no such endpoint exists today, so this means changing both cargo and crates.io.

This is the general fix.
([#16921](https://github.com/rust-lang/cargo/issues/16921)), publish permission
([#16181](https://github.com/rust-lang/cargo/issues/16181)) and rate limits
([#13530](https://github.com/rust-lang/cargo/issues/13530)), which fail the same way.

It is also the hard one. From #16921:

> Likely some new API would need to be worked out between registries and Cargo to check different
> types of information. We'd need some way for this to not cause Cargo to fail on registries that
> don't support it.

**2. Put the list in cargo.**

Cargo already hardcodes other crates.io rules instead of asking the server:
[#8482](https://github.com/rust-lang/cargo/issues/8482) (keyword with a space),
[#6150](https://github.com/rust-lang/cargo/pull/6150) (wildcard versions),
[#7256](https://github.com/rust-lang/cargo/pull/7256) (feature names).

62 names plus lowercase-and-fold matching, checked only when the registry is crates.io. It should
warn, not fail, the same way `--dry-run` already warns for "version already exists".

The downside: the list is data in someone else's database, so cargo's copy can go stale. In practice
it has changed twice in nine years, and both changes added names. A stale copy would therefore miss
a name rather than reject a good one, which is the safer way to be wrong.

**2b. Or publish the list as a file.** crates.io could serve it next to the index. Cargo already
fetches the index during `--dry-run`, so this needs no new API, no auth, and no agreement with other
registries. A registry that does not serve the file is simply not checked.

### Version

Reproduced on current stable and current nightly, so this is not a regression. The `--dry-run` in
the steps above exits 0 on both.

```
cargo 1.99.0 (5f94df478 2026-08-27)
release: 1.99.0
commit-hash: 5f94df4789f005f9a352888e8355ffc645b7ed0e
commit-date: 2026-08-27
host: x86_64-unknown-linux-gnu
libgit2: 1.9.6 (sys:0.21.0 vendored)
libcurl: 8.21.0-DEV (sys:0.4.90+curl-8.21.0 vendored ssl:OpenSSL/3.6.3)
ssl: OpenSSL 3.6.3 9 Jun 2026
os: NixOS 26.5.0 (yarara) [64-bit]
```

```
cargo 1.101.0-nightly (29c5daa1a 2026-10-07)
release: 1.101.0-nightly
commit-hash: 29c5daa1afc262323364e039bb70c83372cc702c
commit-date: 2026-10-07
host: x86_64-unknown-linux-gnu
libgit2: 1.9.6 (sys:0.21.0 vendored)
libcurl: 8.21.0-DEV (sys:0.4.90+curl-8.21.0 vendored ssl:OpenSSL/3.6.3)
ssl: OpenSSL 3.6.3 9 Jun 2026
os: NixOS 26.5.0 (yarara) [64-bit]
```

The code paths quoted above are from `master`, so current cargo behaves the same way. The rejection
itself is server-side, so it does not depend on the cargo version at all.

### Notes

The other problems in this family cost a retry. This one costs the crate name, and it cannot be
undone.

I could not find an existing cargo issue for this. The closest are #16921, #16181, #13530, and the
general [#4377](https://github.com/rust-lang/cargo/issues/4377).
