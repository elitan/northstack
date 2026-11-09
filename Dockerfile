# One image, one binary. Build: docker build -t app .
# Run modes: `app` (all), `app serve`, `app worker`, `app migrate`.

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
COPY crates crates
COPY app app
COPY .sqlx .sqlx
COPY --from=web /src/web/dist web/dist
RUN cargo build --release -p app && strip target/release/app

# --- runtime ----------------------------------------------------------------
# Distroless: no shell, no package manager, runs as non-root.
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/app /app
ENV PORT=3000 LOG_FORMAT=json
EXPOSE 3000
ENTRYPOINT ["/app"]
