# Stage 1: Builder
# This stage compiles both the frontend (WASM) and backend (Axum).
# Pinned to a bookworm base so the binary's glibc matches the bookworm-slim
# runtime. `rust:latest` drifted to a newer Debian (glibc 2.39) and produced a
# binary the runtime (glibc 2.36) could not load — keep builder + runtime on the
# same Debian release.
FROM rust:1-bookworm AS builder

# Install system dependencies needed for compilation
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js for Tailwind CSS build
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y nodejs

# Install wasm32 target and tools for frontend compilation
RUN rustup target add wasm32-unknown-unknown && \
    cargo install wasm-pack

# Set working directory
WORKDIR /app

# Copy entire project
COPY . .

# Install Node dependencies
RUN npm install

# Build Tailwind CSS
RUN npm run tailwind:build

# Build the backend binary
# Output: /app/target/release/backend
RUN cargo build -p backend --release

# Build the frontend WASM with wasm-pack
# This creates optimized WASM in crates/frontend/pkg
RUN cd crates/frontend && \
    wasm-pack build --target web --release

# Stage 2: Runtime
# Use Debian instead of Alpine for libc compatibility
FROM debian:bookworm-slim AS runtime

# Install only runtime dependencies.
# `gringo` provides the `clingo` binary used by the abuse-guardrails engine; the
# install is non-fatal (|| true) so a package-name change never breaks the build —
# the engine degrades to "skipped" (fail-open) if clingo is absent.
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    ca-certificates && \
    (apt-get install -y --no-install-recommends gringo \
     || apt-get install -y --no-install-recommends clingo \
     || echo "WARN: clingo not installed — guardrails will run in skipped mode") && \
    rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy the compiled binary from builder stage
COPY --from=builder /app/target/release/backend /app/backend

# Copy the frontend assets — must match the paths served in main.rs
COPY --from=builder /app/crates/frontend/pkg /app/crates/frontend/pkg
COPY --from=builder /app/crates/frontend/styles.css /app/crates/frontend/styles.css
COPY --from=builder /app/crates/frontend/index.html /app/crates/frontend/index.html
COPY --from=builder /app/crates/frontend/favicon.svg /app/crates/frontend/favicon.svg
COPY --from=builder /app/crates/frontend/favicon.png /app/crates/frontend/favicon.png
# SEO assets — without these the backend's static fallback 404s them in prod.
COPY --from=builder /app/crates/frontend/logo.png /app/crates/frontend/logo.png
COPY --from=builder /app/crates/frontend/og.png /app/crates/frontend/og.png
COPY --from=builder /app/crates/frontend/robots.txt /app/crates/frontend/robots.txt
COPY --from=builder /app/crates/frontend/sitemap.xml /app/crates/frontend/sitemap.xml

# Expose the port the backend listens on
EXPOSE 3001

# Set environment
ENV RUST_LOG=info

# Run the backend server
# The backend will serve the frontend WASM from /app/frontend
CMD ["./backend"]
