FROM rust:1.94-bookworm AS builder
WORKDIR /app

COPY . .

RUN cargo build --release -p osm_pbf_processor

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/osm_pbf_processor /app/osm_pbf_processor

CMD ["./osm_pbf_processor", "--server"]
