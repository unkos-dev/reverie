# Stage 1: Build backend
FROM rust:1-slim AS backend-builder
WORKDIR /build
COPY backend/ .
RUN cargo build --release

# Stage 2: Build frontend
FROM node:22-slim AS frontend-builder
WORKDIR /build
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ .
RUN npm run build

# Stage 3: Runtime
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd -r -s /bin/false tome

COPY --from=backend-builder /build/target/release/tome-api /usr/local/bin/tome-api
COPY --from=frontend-builder /build/dist /srv/frontend

USER tome
EXPOSE 3000
ENTRYPOINT ["tome-api"]
