FROM rust:1.80-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config build-essential clang ca-certificates \
    coinor-libcbc-dev coinor-libclp-dev coinor-libcoinutils-dev coinor-libosi-dev \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY . .

RUN cargo build --release --bin api --manifest-path crates/api/Cargo.toml --features solver-milp/with-milp

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates tzdata curl \
    coinor-libcbc3 coinor-libclp1 coinor-libcoinutils3v5 coinor-libosi1v5 \
 && rm -rf /var/lib/apt/lists/*

RUN useradd -u 10001 -m -s /usr/sbin/nologin app
WORKDIR /app

COPY --from=builder /app/target/release/api /usr/local/bin/api

ENV RUST_LOG=info \
    PORT=8080

EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=2s --retries=10 \
  CMD curl -fsS http://localhost:8080/v1/health || exit 1

USER app
ENTRYPOINT ["/usr/local/bin/api"]
