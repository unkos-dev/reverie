---
severity: medium
surfaces: [ci, security]
adopted: 2026-10-09
adopted-because: the CodeQL Rust extractor builds build scripts and procedural macros with its own fixed toolchain, which is older than the declared `rust-version`, so Cargo refuses the build and the scan continues without macro expansion
lift-when-class: dep-unblocks
lift-when: a supported upstream mechanism handles projects whose declared minimum exceeds the extractor's preferred toolchain, and extraction without this workaround demonstrably preserves the required macro coverage
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

Remove when a supported upstream mechanism handles projects whose declared minimum exceeds the extractor's preferred
toolchain, and extraction without this workaround demonstrably preserves the required macro coverage. The environment
variable is then deleted.

## Related

- `.github/workflows/codeql.yml`
- `backend/Cargo.toml`
