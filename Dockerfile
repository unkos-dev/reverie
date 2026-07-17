# syntax=docker/dockerfile:1.25@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12

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
FROM rust:1-slim-trixie@sha256:34fb2f168c432d421a09883c663b275b33cbb30f6b18642fbd09a684c6546d0e AS chef
RUN cargo install cargo-chef@0.1.77 --locked
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
RUN cargo chef cook --release --recipe-path recipe.json

# Stage 1d: backend-builder — real build atop warm dep layer.
# SQLX_OFFLINE forces sqlx::query! macros to validate against the committed
# .sqlx/ cache instead of opening a database connection at compile time.
# Cache regeneration: `cargo sqlx prepare -- --tests` against a populated dev DB.
FROM cooker AS backend-builder
COPY backend/ .
ENV SQLX_OFFLINE=true
RUN cargo build --release

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
FROM node:24.18.0-slim@sha256:6f7b03f7c2c8e2e784dcf9295400527b9b1270fd37b7e9a7285cf83b6951452d AS frontend-builder
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

# Stage 3: Runtime
# UNK-253: codename MUST match the builder stage above. See note on `chef`.
FROM debian:trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd AS runtime
# UNK-165: curl is the HTTP client used by the HEALTHCHECK below; readiness
# probe needs a working HTTP client baked in so docker / compose / Incus can
# detect when the server is up and the schema check has passed before
# flipping traffic.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
RUN useradd -r -s /bin/false reverie

COPY --from=backend-builder /build/target/release/reverie-api /usr/local/bin/reverie-api
COPY --from=frontend-builder /build/frontend/dist /srv/frontend
# UNK-106: the backend serves /assets/* and falls back to index.html for SPA
# routes when this env var is set. Validation at startup panics the process
# if the dir or its csp-hashes.json sidecar is missing.
ENV REVERIE_FRONTEND_DIST_PATH=/srv/frontend

USER reverie
EXPOSE 3000

# UNK-165: probe the readiness endpoint (DB-dependent) so the container is
# only reported healthy once the startup schema check passes and the pool is
# live. The default entrypoint verifies the schema (it does not migrate); run
# `reverie migrate` first, or set REVERIE_AUTO_MIGRATE=true (then the
# start-period must also cover the in-process migration on first boot).
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 \
    CMD curl --fail --silent --show-error --output /dev/null http://127.0.0.1:3000/health/ready

ENTRYPOINT ["reverie-api"]
