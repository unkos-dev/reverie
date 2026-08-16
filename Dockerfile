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
FROM rust:1-slim-trixie@sha256:8e8cf8f7fd54a2d23d5a743b3a03f56e26b6c774276c33fa0595111704ebb15c AS chef
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

# Stage 2: Build frontend with buildkit npm cache mount.
# The mount avoids re-fetching tarballs when the npm-ci layer cache is invalidated
# but the lockfile is unchanged — buildkit reuses the mount within a single
# build. GHA runners are ephemeral so the mount does not persist across runs;
# cross-run npm reuse is provided by the gha layer cache instead.
#
# npm workspaces: the lockfile is hoisted to the repo root, so the install
# needs the root manifests plus every workspace manifest to resolve the
# workspace graph. docs/package.json is present for graph resolution only —
# `--workspace frontend` keeps its dependencies out of the install.
# --ignore-scripts skips the root `prepare` hook (lefthook install), which
# requires a .git directory the build context deliberately excludes; the
# frontend build tools ship platform binaries as scriptless optional deps,
# so nothing in the install relies on lifecycle scripts.
FROM node:24.19.0-slim@sha256:3638d9a6fe4030bd716be989438248074489337ba3275657f93595428be4fc03 AS frontend-builder
# vp's native binary initializes an HTTPS client at startup and aborts when
# the system has no CA store; the slim base ships none.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY package.json package-lock.json ./
COPY frontend/package.json frontend/
COPY docs/package.json docs/
# devEngines pins npm exactly, and the base image's bundled npm lags it —
# the `onFail: download` self-swap does not fire under `npm ci` here, it
# just hard-fails (EBADDEVENGINES). Install the manifest-pinned npm first,
# reading the version from package.json so this line can never drift from
# the pin it exists to satisfy.
RUN --mount=type=cache,target=/root/.npm npm install -g "npm@$(node -p "require('./package.json').devEngines.packageManager.version")"
RUN --mount=type=cache,target=/root/.npm npm ci --workspace frontend --ignore-scripts
COPY frontend/ frontend/
RUN npm run build --workspace frontend

# Stage 2b: frontend-sbom — dependency record for the bundled frontend.
# The frontend ships as a bundle, so its dependencies are not packages in
# the runtime image and no image scanner can recover them. Emitting the
# list from the tree that produced the bundle keeps the record from
# drifting. --omit=dev because this describes what ships, not what built
# it.
#
# Separate stage because `npm sbom` validates the whole workspace tree and
# aborts with ESBOMPROBLEMS on any absent member, while the build above
# deliberately installs only the frontend workspace. The full install
# lives here so the builder stage keeps its narrow install and the
# runtime image never sees either.
#
# npm sbom also refuses when a workspace member lacks a version, and the
# private root has none (EINVALIDPURLTYPE). Borrow the release version
# for generation alone: this manifest is a build artefact that is never
# published, so release-please stays the tree's only version authority.
FROM frontend-builder AS frontend-sbom
COPY version.txt ./
RUN --mount=type=cache,target=/root/.npm npm ci --ignore-scripts \
    && npm pkg set version="$(cat version.txt)" \
    && npm sbom --sbom-format cyclonedx --omit=dev --workspace frontend \
       > /build/frontend.cdx.json

# Stage 3: Runtime
# UNK-253: codename MUST match the builder stage above. See note on `chef`.
FROM debian:trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd AS runtime
# UNK-165: curl is the HTTP client used by the HEALTHCHECK below; readiness
# probe needs a working HTTP client baked in so docker / compose / Incus can
# detect when the server is up and the schema check has passed before
# flipping traffic.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
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
