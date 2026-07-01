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

# Install ca-certificates (required for HTTPS connections to PSE website)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /usr/src/phisix-rust/target/release/phisix-rust /app/phisix-rust

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
