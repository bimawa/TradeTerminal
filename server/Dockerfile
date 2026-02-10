FROM rust:latest AS builder

WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock* ./
COPY shared ./shared
COPY server ./server
COPY client ./client

RUN cargo build --release --package trade-server

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/trade-server /usr/local/bin/

ENV RUST_LOG=info
ENV LISTEN_ADDR=0.0.0.0:9000

EXPOSE 9000

CMD ["trade-server"]
