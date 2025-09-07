FROM rust:1 as builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev

COPY . .
RUN cargo build --release -p api

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && update-ca-certificates
WORKDIR /srv
COPY --from=builder /app/target/release/api /srv/unischedule-api
ENV UNISCHEDULE__SERVER__PORT=8080 RUST_LOG=info
EXPOSE 8080
CMD ["/srv/unischedule-api"]
