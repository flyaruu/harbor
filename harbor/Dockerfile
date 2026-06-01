FROM rust:1.95-bookworm AS builder
WORKDIR /app

# Keep the wasm release build single-threaded and without debug symbols to
# reduce peak memory use on smaller hosts.
ENV CARGO_BUILD_JOBS=1
ENV RUSTFLAGS=-Cdebuginfo=0 -Ccodegen-units=1

RUN rustup target add wasm32-unknown-unknown \
    && apt-get update \
    && apt-get install -y --no-install-recommends binaryen \
    && rm -rf /var/lib/apt/lists/* \
    && cargo install wasm-bindgen-cli --version 0.2.121

COPY . .

RUN cargo build --release --target wasm32-unknown-unknown -p harbor \
    && wasm-opt -Os \
        -o /app/target/wasm32-unknown-unknown/release/harbor.opt.wasm \
        /app/target/wasm32-unknown-unknown/release/harbor.wasm \
    && mkdir -p /app/harbor/dist \
    && wasm-bindgen \
        --target web \
        --out-dir /app/harbor/dist \
        /app/target/wasm32-unknown-unknown/release/harbor.opt.wasm

FROM debian:bookworm-slim
WORKDIR /srv/harbor

RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY harbor/container-index.html /srv/harbor/index.html
COPY harbor/container-entrypoint.sh /usr/local/bin/harbor-container-entrypoint
COPY --from=builder /app/harbor/dist /srv/harbor
COPY --from=builder /app/harbor/assets /srv/harbor/assets

RUN chmod +x /usr/local/bin/harbor-container-entrypoint

EXPOSE 1334

CMD ["/usr/local/bin/harbor-container-entrypoint"]
