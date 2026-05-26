# Stage 1: Builder
# This stage compiles both the frontend (WASM) and backend (Axum)
FROM rust:latest AS builder

# Install system dependencies needed for compilation
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    nodejs \
    npm \
    && rm -rf /var/lib/apt/lists/*

# Install wasm32 target for frontend compilation
RUN rustup target add wasm32-unknown-unknown

# Install cargo-leptos CLI for full-stack build
RUN cargo install cargo-leptos

# Set working directory
WORKDIR /app

# Copy entire project
COPY . .

# Build the full-stack app (frontend WASM + backend binary)
# cargo-leptos handles both frontend and backend compilation
# Output: /app/target/site/ (frontend WASM + static assets)
#         /app/target/release/backend (backend binary)
RUN cargo leptos build --release

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

# Copy the compiled frontend (WASM + assets) from builder stage
COPY --from=builder /app/target/site /app/site

# Expose the port the backend listens on
EXPOSE 3001

# Set environment
ENV RUST_LOG=info

# Run the backend server
# The backend will serve the frontend static files from /app/site
CMD ["./backend"]
