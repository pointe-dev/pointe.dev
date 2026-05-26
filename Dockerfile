# Stage 1: Builder
# This stage compiles both the frontend (WASM) and backend (Axum)
FROM rust:latest AS builder

# Install system dependencies needed for compilation
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install wasm32 target and tools for frontend compilation
RUN rustup target add wasm32-unknown-unknown && \
    cargo install wasm-pack

# Set working directory
WORKDIR /app

# Copy entire project
COPY . .

# Build the backend binary
# Output: /app/target/release/backend
RUN cargo build -p backend --release --locked

# Build the frontend WASM with wasm-pack
# This creates optimized WASM in crates/frontend/pkg
RUN cd crates/frontend && \
    wasm-pack build --target web --release

# Stage 2: Runtime
# This is a lightweight image that only contains what's needed to run the app
FROM alpine:3.18 AS runtime

# Install only runtime dependencies (OpenSSL)
RUN apk add --no-cache \
    libssl3 \
    ca-certificates

# Create app directory
WORKDIR /app

# Copy the compiled binary from builder stage
COPY --from=builder /app/target/release/backend /app/backend

# Copy the frontend WASM from builder stage
COPY --from=builder /app/crates/frontend/pkg /app/frontend

# Expose the port the backend listens on
EXPOSE 3001

# Set environment
ENV RUST_LOG=info

# Run the backend server
# The backend will serve the frontend WASM from /app/frontend
CMD ["./backend"]
