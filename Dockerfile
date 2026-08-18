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
# drifting.
#
# Scanning the hoisted root package-lock.json directly (no install) was
# considered and rejected: npm workspace lockfiles flatten every member's
# dependencies into one unattributed list, so a lockfile-only catalog of
# this tree would also pull in docs/'s production dependencies (astro,
# sharp, ...) with no way to tell them apart from the frontend's own.
# `npm ci --omit=dev --workspace frontend` installs the frontend's
# production tree dev-free, so no syft dev/prod heuristic is needed.
# Known over-inclusion: npm's workspace install also leaks a few of the
# root's own installs (esbuild, typescript, the vite-plus core) into
# node_modules, so those appear in the document as if shipped —
# over-reporting, the safe direction for an SBOM. The pnpm migration's
# filtered install removes the leak. Separate stage so this
# install (and the syft binary used to read it) never reach the runtime
# image; the builder stage above keeps its own narrower dev+prod install
# for the build tools it needs.
FROM frontend-builder AS frontend-sbom
# curl is needed only to fetch the pinned syft binary below; not present
# in the node:slim base.
RUN apt-get update && apt-get install -y --no-install-recommends curl \
    && rm -rf /var/lib/apt/lists/*
ARG TARGETARCH
# renovate: datasource=github-release-attachments depName=anchore/syft extractVersion=^v(?<version>.+)$
ARG SYFT_VERSION=1.51.0
# Checksums are the official ones published in syft's
# syft_<version>_checksums.txt release asset. Renovate bumps SYFT_VERSION
# via the annotation above but cannot recompute a checksum, so a version
# bump fails this RUN until a reviewer pastes the matching sha256 pair.
RUN set -eu; \
    case "$TARGETARCH" in \
      amd64) sha256=2a2e837a2c8d59ec9af5472ee22d3b04ee463c4e44476ecf993fd1e5ab6ebc7f ;; \
      arm64) sha256=6c0466811541ea03add5213a60a1562f0851e4c0b0ecfdee1a694a9455285900 ;; \
      *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    curl -fsSL --connect-timeout 10 --max-time 300 --retry 2 --retry-all-errors \
      -o /tmp/syft.tar.gz \
      "https://github.com/anchore/syft/releases/download/v${SYFT_VERSION}/syft_${SYFT_VERSION}_linux_${TARGETARCH}.tar.gz"; \
    printf '%s  /tmp/syft.tar.gz\n' "$sha256" > /tmp/syft.tar.gz.sha256; \
    sha256sum -c /tmp/syft.tar.gz.sha256; \
    tar -xzf /tmp/syft.tar.gz -C /usr/local/bin syft; \
    rm -f /tmp/syft.tar.gz /tmp/syft.tar.gz.sha256
# --omit=dev scopes the install (and therefore the catalog) to production
# dependencies; javascript-package-cataloger reads installed package.json
# files with no dev/prod distinction of its own, so what is on disk is
# what ends up in the document.
#
# javascript-package-cataloger is tagged for image/installed sources, not
# directory sources, so syft excludes it from the default set for a
# `dir:` scan; --override-default-catalogers forces it in and drops every
# other default (go-module and github-actions-usage catalogers otherwise
# fire on binaries and workflow-shaped fixtures bundled inside some
# packages, which are noise for a dependency record).
RUN --mount=type=cache,target=/root/.npm npm ci --omit=dev --workspace frontend --ignore-scripts \
    && syft dir:node_modules --override-default-catalogers javascript-package-cataloger \
       -o cyclonedx-json > /build/frontend.cdx.json

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
