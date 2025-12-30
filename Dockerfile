FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update && apt-get install -y musl-tools
RUN rustup target add x86_64-unknown-linux-musl
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl -p cli

# ----------- Runtime Stage -----------
FROM alpine:latest

# Create a non-root user with UID 1000 (common first user UID)
RUN addgroup -g 1000 appgroup && adduser -u 1000 -G appgroup -s /bin/sh -D appuser

# Create data directory with proper permissions
RUN mkdir -p /app/data /app/static && chown -R appuser:appgroup /app

# Copy binary and frontend
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/cli /usr/local/bin/cli
COPY --from=ghcr.io/jorgecarleitao/stv-app-frontend:main /app/dist /app/static

RUN chown appuser:appgroup /usr/local/bin/cli

USER appuser
WORKDIR /app/data

# Set default environment variables
ENV DATABASE_URL="sqlite:///app/data/elections.db?mode=rwc"
ENV FRONTEND_STATIC_DIR="/app/static"

EXPOSE 8080

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/cli"]
