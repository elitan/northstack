# One image, one binary. Build: docker build -t app .
# Run: docker run -p 3000:3000 -v app-data:/data app
# Modes: `app` (all), `app serve`, `app worker`, `app migrate`.
#
# The database is one SQLite file, /data/app.db. Mount a persistent volume at
# /data on a local disk (not a network file system).

# --- frontend ---------------------------------------------------------------
FROM oven/bun:1 AS web
WORKDIR /src/web
COPY web/package.json web/bun.lock ./
RUN bun install --frozen-lockfile
COPY web/ ./
RUN bun run build

# --- backend ----------------------------------------------------------------
FROM rust:1-bookworm AS build
WORKDIR /src
# Queries are checked against the committed .sqlx cache; no database needed.
ENV SQLX_OFFLINE=true
COPY Cargo.toml Cargo.lock ./
# Build settings (SQLite compile flags for concurrency).
COPY .cargo .cargo
COPY crates crates
COPY server server
COPY .sqlx .sqlx
COPY --from=web /src/web/dist web/dist
RUN cargo build --release -p server && strip target/release/app
# An empty /data owned by the runtime user, so it works without a volume too.
RUN mkdir -p /data

# --- runtime ----------------------------------------------------------------
# Distroless: no shell, no package manager, runs as non-root (uid 65532).
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/app /app
COPY --from=build --chown=65532:65532 /data /data
ENV PORT=3000 LOG_FORMAT=json DATABASE_URL=sqlite:/data/app.db
VOLUME /data
EXPOSE 3000
ENTRYPOINT ["/app"]
