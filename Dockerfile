# Stage 1: Builder
# This stage compiles both the frontend (WASM) and backend (Axum)
FROM rust:latest as builder

# Install system dependencies needed for compilation
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install wasm32 target for frontend compilation
RUN rustup target add wasm32-unknown-unknown

# Set working directory
WORKDIR /app

# Copy entire project
COPY . .

# Build the backend (Axum binary) in release mode
# This produces an optimized binary at target/release/backend
RUN cargo build -p backend --release --locked

# Note: Frontend WASM is built by Leptos as part of the backend build
# The WASM output is placed in target/site/ by cargo-leptos

# Stage 2: Runtime
# This is a lightweight image that only contains what's needed to run the app
FROM alpine:3.18

# Install only runtime dependencies (OpenSSL)
RUN apk add --no-cache \
    libssl3 \
    ca-certificates

# Create app directory
WORKDIR /app

# Copy the compiled binary from builder stage
COPY --from=builder /app/target/release/backend /app/backend

# Copy the compiled frontend (WASM + assets) from builder stage
COPY --from=builder /app/target/site /app/site

# Expose the port the backend listens on
EXPOSE 3001

# Set environment
ENV RUST_LOG=info

# Run the backend server
# The backend will serve the frontend static files from /app/site
CMD ["./backend"]
