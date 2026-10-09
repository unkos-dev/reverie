---
severity: medium
surfaces: [ci, security]
adopted: 2026-10-09
adopted-because: the CodeQL Rust extractor builds build scripts and procedural macros with its own fixed toolchain, which is older than the declared `rust-version`, so Cargo refuses the build and the scan continues without macro expansion
lift-when-class: dep-unblocks
lift-when: CodeQL Rust extraction builds the backend's build scripts and procedural macros without the extra argument, with `rust-version` in backend/Cargo.toml unchanged and first-party macro expansion restored
---

# CodeQL Rust extraction ignores the declared Rust version

## Constraint

The CodeQL Rust extractor sets `RUSTUP_TOOLCHAIN` for every Cargo command it runs to a fixed toolchain chosen for its
bundled rust-analyzer (`FIXED_RUST_TOOLCHAIN` in the extractor's `toolchain.rs`), and applies it after any user-supplied
environment. That toolchain trails the latest stable release, while `rust-version` in `backend/Cargo.toml` tracks the
pinned toolchain.

When the declared version is newer than the extractor's toolchain, Cargo refuses to build the package. rust-analyzer
records the failure at debug level and loads the workspace without build-script outputs or procedural-macro expansions.
The scan still succeeds, so `sqlx` query macros, derives and attribute macros drop out of the analysed code with no
failing check.

## Workaround

The Rust CodeQL job sets `CODEQL_EXTRACTOR_RUST_OPTION_CARGO_EXTRA_ARGS` to `--ignore-rust-version`. rust-analyzer
passes extra arguments to the `cargo check` that runs build scripts and filters them out of `cargo metadata`, so the
override applies to extraction only and the manifest stays accurate.

## Why this isn't the right shape

`cargo_extra_args` is implemented by the extractor but absent from its published option schema, so an extractor update
can drop or change it without notice. The flag also only skips Cargo's version check: a build script or procedural macro
that genuinely needs a newer compiler still fails, again without failing the scan.

## Lift conditions

CodeQL extraction builds the backend's build scripts and procedural macros without the extra argument, with
`rust-version` unchanged and first-party macro expansion restored. The environment variable is then deleted.

## Related

- `.github/workflows/codeql.yml`
- `backend/Cargo.toml`
