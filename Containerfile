# Stage 1: Build the application
FROM rust:slim AS builder

WORKDIR /usr/src/phisix-rust

# Install dependencies needed for compilation (e.g. build-essential)
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Copy source code
COPY . .

# Build for release
RUN cargo build --release

# Stage 2: Create a minimal runtime image
FROM debian:bookworm-slim

# ca-certificates: kept for belt and braces, though reqwest is built with
# webpki-roots here so the TLS trust store is actually compiled into the binary.
#
# curl: required by the container HEALTHCHECK. A healthcheck runs INSIDE the
# container, and debian-slim has neither curl nor wget -- so a
# `HealthCmd=curl ...` in the quadlet fails with `curl: not found` on every
# probe and the container sits in "starting" forever while podman logs a
# failing streak. Found exactly that way on 2026-08-28. It also makes the image
# debuggable from the inside, which is worth the ~6 MB.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /usr/src/phisix-rust/target/release/phisix-rust /app/phisix-rust

# Copy the static assets. main.rs serves these with ServeDir::new("static"),
# which is relative to the working directory — so with WORKDIR /app they must
# land at /app/static. Without this the fallback service 404s every static
# path (favicons, robots.txt) even though the files exist in the repo.
COPY --from=builder /usr/src/phisix-rust/static /app/static

# Create database directory
RUN mkdir -p /app/data

# Expose server port
EXPOSE 8080

# Environment variables
ENV PORT=8080
ENV DATABASE_PATH=/app/data/phisix.db
ENV RUST_LOG=phisix_rust=info,tower_http=info

# Run the binary
CMD ["/app/phisix-rust"]
