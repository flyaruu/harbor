FROM rust:1.95-bookworm AS builder
WORKDIR /app

COPY . .

RUN cargo build --release -p location-source

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/location-source /app/location-source

CMD ["./location-source"]
