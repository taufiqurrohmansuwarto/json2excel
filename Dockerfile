# Multi-stage build untuk optimasi ukuran image
FROM rust:latest AS builder

# Install dependencies yang dibutuhkan untuk compile
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    build-essential \
    libclang-dev \
    clang \
    musl-tools \
    && rm -rf /var/lib/apt/lists/*

# Add musl target for static linking
RUN rustup target add x86_64-unknown-linux-musl

# Set working directory
WORKDIR /app

# Copy manifests
COPY Cargo.toml ./

# Copy source code
COPY src ./src

# Build for release with musl target for static linking
RUN cargo build --release --target x86_64-unknown-linux-musl

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create app user
RUN useradd -r -s /bin/false appuser

# Set working directory
WORKDIR /app

# Copy binary from builder stage
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/excel-service ./excel-service

# Change ownership
RUN chown appuser:appuser /app/excel-service

# Switch to non-root user
USER appuser

# Expose port
EXPOSE 3333

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3333/health || exit 1

# Run the application
CMD ["./excel-service"]