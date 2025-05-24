# Multi-stage build for optimized production image
FROM rust:1.87-slim AS builder

# Install build dependencies for Debian/Ubuntu
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy dependency files first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies (this layer will be cached unless Cargo.toml changes)
RUN cargo build --release && rm -rf src

# Copy the actual source code
COPY src ./src

# Build the application
# Touch main.rs to ensure it's rebuilt with the new source
RUN touch src/main.rs && cargo build --release

# Runtime stage - use Google's distroless image with SSL support for maximum security
FROM gcr.io/distroless/cc-debian12:nonroot

# Copy the binary from builder stage
COPY --from=builder /app/target/release/Nginx-Cloudflare-Access-JWT-Validator /usr/local/bin/Nginx-Cloudflare-Access-JWT-Validator

# Set default log level to info
ENV RUST_LOG=info

# Expose the port
EXPOSE 8080

# Run the application
ENTRYPOINT ["/usr/local/bin/Nginx-Cloudflare-Access-JWT-Validator"]
