# Multi-stage Dockerfile for Thermos-Rust
# Builds optimized binary for Linux (Debian Trixie)

# Build stage
FROM rust:slim-trixie as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /build

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:trixie-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3t64 \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/target/release/thermos-rust /app/thermos-rust

# Create config directory
RUN mkdir -p /app/config

# Expose ports
EXPOSE 5540/udp
EXPOSE 8080/tcp

# Set environment
ENV RUST_LOG=info

# Run as root to access GPIO
USER root

# Run the application
CMD ["/app/thermos-rust"]
