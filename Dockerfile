FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN rustup target add x86_64-unknown-linux-musl
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl -p cli

# ----------- Runtime Stage -----------
FROM alpine:latest

# Create a non-root user and group
RUN addgroup -S appgroup && adduser -S appuser -G appgroup

# Set permissions and copy the binary
USER appuser
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/cli /usr/local/bin/cli
COPY --from=ghcr.io/jorgecarleitao/stv-app-frontend:main /app/dist /app/static

EXPOSE 8080

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/cli"]
