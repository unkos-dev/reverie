# syntax=docker/dockerfile:1.26@sha256:ecfaec9ed6d810b56388c508f4121597bfbba70d41a6dfeee4d8cad5f295fc32

# Stage 1a: chef base — pinned cargo-chef install shared across planner + cooker.
# Version pin prevents recipe.json schema drift between planner emit and cooker
# consume. Bump in lockstep across both stages (they inherit from this base).
#
# UNK-253: Debian codename pinned explicitly (-trixie) and MUST match the
# runtime stage codename below. Unpinned `rust:1-slim` follows upstream's
# default codename, which silently flipped bookworm → trixie and broke ARM64
# `:main` images with `GLIBC_2.38 not found` against a bookworm runner. Both
# stages share the same codename so the dynamic linker can resolve every
# symbol the release binary requests.
FROM rust:1-slim-trixie@sha256:cc0448b41c3b7b7fea44f5dc50eacba729a56db365b65b7bd5e8a82d5b3db078 AS chef
# cargo-auditable embeds the resolved dependency list into the release
# binary. Without it the published SBOM is silent about every crate,
# because the runtime image holds a compiled binary rather than
# installed packages; syft and trivy both recover the embedded list.
RUN cargo install cargo-chef@0.1.77 --locked \
    && cargo install cargo-auditable@0.7.5 --locked
WORKDIR /build

# Stage 1b: planner — emits recipe.json describing the dependency tree.
# Cheap stage (no compilation); recipe.json hash drives cooker cache key.
FROM chef AS planner
COPY backend/ .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 1c: cooker — compiles deps only, from recipe.json.
# This layer is the cache target — warm hits skip ~3min of dep compilation.
FROM chef AS cooker
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json

# Stage 1d: backend-builder — real build atop warm dep layer.
# SQLX_OFFLINE forces sqlx::query! macros to validate against the committed
# .sqlx/ cache instead of opening a database connection at compile time.
# Cache regeneration: `cargo sqlx prepare -- --tests` against a populated dev DB.
FROM cooker AS backend-builder
COPY backend/ .
ENV SQLX_OFFLINE=true
# `auditable build` is the plain build plus a dependency manifest linked
# into the binary; only the final link needs it, so the cooked dep layer
# above is unaffected and stays cache-valid.
RUN cargo auditable build --release --locked

# Stage 2a: js-toolchain — the shared base for both JS stages. pnpm plus the
# Node runtime it provisions, and nothing project-specific, so the two stages
# below branch from it and run concurrently instead of one waiting on the other.
#
# The base is pnpm's own published image, per https://pnpm.io/docker. It is
# Debian trixie — the same codename as the runtime stage, which UNK-253 below
# requires — and it carries the pnpm standalone binary, a CA store and
# libatomic.so.1. That replaces a GPG-verified mise installer, two apt packages
# and a bootstrap that existed only to obtain pnpm.
#
# The image bundles no Node at all, so the runtime is installed explicitly.
# `pnpm install` does not do it: neither a filtered nor an unfiltered install
# leaves a `node` behind, because `devEngines.runtime` declares the version
# without provisioning it. `pnpm runtime set` provisions it, and once that has
# run `node` is on PATH at /pnpm/bin/node like any other interpreter.
#
# The version is read back out of package.json rather than written here.
# Supplying it literally would put a second Node pin in the tree, which is the
# thing devEngines.runtime exists to remove; omitting it entirely is worse,
# because `pnpm runtime set node -g` with no version installs the latest
# release and silently ignores the declaration. `pnpm pkg get` needs no Node of
# its own, so it works before any runtime exists.
FROM ghcr.io/pnpm/pnpm:11@sha256:eba76954b37ec1ba6187f0adb39caee1e31733194857eedd01319da0af3fa00d AS js-toolchain
WORKDIR /build
# pnpm resolves the workspace graph from the root manifests plus every project
# manifest, and reads its own version from packageManager, so no line below can
# drift from a pin it exists to satisfy.
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY frontend/package.json frontend/
COPY docs/package.json docs/
# Installed once here so both stages below inherit one runtime from this layer
# rather than provisioning it twice.
#
# `runtime set -g` uses pnpm's global project, not the repository lockfile.
# The workspace fetch below validates committed entries but does not provision
# the Node executable used by this build.
RUN pnpm runtime set node "$(pnpm pkg get devEngines.runtime.version)" -g \
    && node --version
ENV pnpm_config_store_dir=/pnpm-store

# GHA does not preserve cache mounts, so required packages belong in a layer.
RUN pnpm fetch

# Stage 2b: Build the frontend bundle.
#
# Workspace resolution needs every copied manifest; the filter excludes docs.
# `--ignore-scripts` is defence in depth after the workspace allowlist.
FROM js-toolchain AS frontend-builder
RUN pnpm install --offline --frozen-lockfile --filter frontend... --ignore-scripts
COPY frontend/ frontend/
# Source timestamps can trigger pnpm 11's unscoped repair install. The explicit
# frozen offline install above owns dependency validation for this stage.
RUN PNPM_CONFIG_VERIFY_DEPS_BEFORE_RUN=false pnpm --filter frontend run build

# Stage 2c: frontend-sbom — the dependency record for the bundled frontend. The
# runtime image holds no packages for a scanner to find, so the document is
# generated here and copied in.
#
# `deploy` materialises the production closure instead of leaving store links
# that a directory scan cannot follow. Syft's default dir scan excludes its
# JavaScript cataloger, so it must be selected explicitly.
FROM js-toolchain AS frontend-sbom
# renovate: datasource=docker depName=anchore/syft
COPY --from=anchore/syft:v1.51.0@sha256:678bfa565b60f747aac0f8e964fe5588a24445b8d0a480e91f6efd70020dfbb0 /syft /usr/local/bin/syft
RUN pnpm deploy --ignore-scripts --filter frontend --prod /sbom-tree \
    && syft dir:/sbom-tree --override-default-catalogers javascript-package-cataloger \
      -o cyclonedx-json > /build/frontend.cdx.json

# Consistency check for the document above. It records a verdict and never
# fails the build: the SBOM is a published deliverable, not a security control
# (Snyk and trivy scan the source and the image directly), so a defect here
# must not block a release. The publish workflow's per-digest verification step
# reads this verdict once the image is pushed and opens an issue, which is what
# stops a broken SBOM going unnoticed. The script carries the reasoning for
# what it compares and why.
COPY scripts/verify-frontend-sbom.mjs /usr/local/lib/verify-frontend-sbom.mjs
RUN node /usr/local/lib/verify-frontend-sbom.mjs /sbom-tree /build/frontend.cdx.json \
      > /build/sbom-verify.txt \
    && cat /build/sbom-verify.txt

# Stage 3: Runtime
# UNK-253: codename MUST match the builder stage above. See note on `chef`.
FROM debian:trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd AS runtime
# UNK-165: curl is the HTTP client used by the HEALTHCHECK below; readiness
# probe needs a working HTTP client baked in so docker / compose / Incus can
# detect when the server is up and the schema check has passed before
# flipping traffic.
# The upgrade is standing design, not a patch: Debian's apt repo carries
# security fixes days-to-weeks before the base image rebuilds with them,
# and the publish gate fails on fixable CVEs, so every build applies the
# suite's current fixes on top of the pinned base.
RUN apt-get update && apt-get upgrade -y \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
# Fixed numeric id rather than letting `useradd -r` pick one: the allocation
# is whatever the base image happens to have free, so it can move under the
# running container between base-image rebuilds, and an orchestrator that
# must prove the process is non-root before admitting it cannot resolve a
# name at all. No volume is mounted as this user, so the id is free to pin.
RUN useradd -r -u 10001 -s /bin/false reverie

COPY --from=backend-builder /build/target/release/reverie-api /usr/local/bin/reverie-api
COPY --from=frontend-builder /build/frontend/dist /srv/frontend
# Load-bearing, not merely informational: syft ingests SBOM documents it
# finds in the filesystem, so placing this here folds the bundled frontend
# dependencies into the image's own SBOM attestation, which no filesystem
# scan could otherwise recover. The attestation then covers all three
# ecosystems in one document and needs no second release asset. Syft
# records the provenance as "acquired package info from SBOM: <path>".
# It also lets an operator read the list straight off the running image.
COPY --from=frontend-sbom /build/frontend.cdx.json /usr/share/reverie/sbom/frontend.cdx.json
# The verdict travels with the document it describes: the check runs where both
# the SBOM and pnpm's resolved closure exist, but the thing that must notice a
# failure is a workflow step with access to neither. Reading it back off each
# published digest is what closes that gap.
COPY --from=frontend-sbom /build/sbom-verify.txt /usr/share/reverie/sbom/verify.txt
# UNK-106: the backend serves /assets/* and falls back to index.html for SPA
# routes when this env var is set. Validation at startup panics the process
# if the dir or its csp-hashes.json sidecar is missing.
ENV REVERIE_FRONTEND_DIST_PATH=/srv/frontend

USER 10001
EXPOSE 3000

# UNK-165: probe the readiness endpoint (DB-dependent) so the container is
# only reported healthy once the startup schema check passes and the pool is
# live. The default entrypoint verifies the schema (it does not migrate); run
# `reverie migrate` first, or set REVERIE_AUTO_MIGRATE=true (then the
# start-period must also cover the in-process migration on first boot).
# Exec form: the probe needs no shell feature, so the shell form would only
# add a `/bin/sh -c` process between the runtime and curl on every interval.
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 \
    CMD ["curl", "--fail", "--silent", "--show-error", \
         "--output", "/dev/null", "http://127.0.0.1:3000/health/ready"]

ENTRYPOINT ["reverie-api"]
