# Ember - AI Agent Framework
# Multi-stage build for minimal image size

# Stage 1: Build
FROM rust:1.83-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binary
RUN cargo build --release -p ember-cli

# Stage 2: Runtime
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 ember

# Copy binary from builder
COPY --from=builder /app/target/release/ember-cli /usr/local/bin/ember

# Set permissions
RUN chmod +x /usr/local/bin/ember

# Switch to non-root user
USER ember
WORKDIR /home/ember

# Create config directory
RUN mkdir -p .config/ember

# Default environment
ENV EMBER_PROVIDER=ollama
ENV OLLAMA_HOST=http://host.docker.internal:11434

# Healthcheck
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ember --version || exit 1

# Entry point
ENTRYPOINT ["ember"]
CMD ["--help"]

# Labels
LABEL org.opencontainers.image.title="Ember"
LABEL org.opencontainers.image.description="Blazing-fast AI agent framework written in Rust"
LABEL org.opencontainers.image.source="https://github.com/ember-ai/ember"
LABEL org.opencontainers.image.licenses="MIT OR Apache-2.0"