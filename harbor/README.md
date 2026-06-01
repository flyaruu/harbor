# Harbor

## SpacetimeDB bindings

Regenerate the Rust client bindings from the sibling `ship-spacetime` module with:

```bash
spacetime generate --lang rust --out-dir "/Users/flyaruu/git/ship/harbor/src/module_bindings" --module-path "/Users/flyaruu/git/ship/ship-spacetime/spacetimedb"
```

By default, the app connects to:

- `SPACETIMEDB_URI=http://localhost:3000`
- `SPACETIMEDB_MODULE=ship-spacetime`
- `TILE_SERVER_URI=http://localhost:8081`

Set `SPACETIMEDB_TOKEN` as well if the target database requires authentication.

On native/desktop, map tiles are fetched over HTTP from `TILE_SERVER_URI` and cached under:

```text
.cache/harbor_tiles/
```

Tiles are loaded around the active camera focus instead of preloading a fixed tile manifest at startup.

## Running

Desktop:

```bash
cargo run
```

To override the native SpacetimeDB URL for a single run, pass `--url` after
the Cargo separator:

```bash
cargo run -p harbor -- --url http://localhost:3000
```

To override the native tile server URL for a single run:

```bash
cargo run -p harbor -- --tile-url http://localhost:8081
```

Native config precedence for the SpacetimeDB URI is:

1. `--url`
2. `SPACETIMEDB_URI`
3. `http://localhost:3000`

Native config precedence for the tile server URI is:

1. `--tile-url`
2. `TILE_SERVER_URI`
3. `http://localhost:8081`

## Performance Tooling

- An FPS and frame-time overlay is shown in the bottom-right corner by default.
- For per-system and per-frame timing breakdowns, run Harbor with Bevy's Chrome trace instrumentation enabled:

```bash
cargo run -p harbor --features chrome_trace
```

Then open the generated trace in [Perfetto](https://ui.perfetto.dev/) or `chrome://tracing`.

Wasm/browser:

1. Install the wasm target:

```bash
rustup target add wasm32-unknown-unknown
```

2. Install the runner once:

```bash
cargo install wasm-server-runner
```

3. Run in the browser:

```bash
cargo run-wasm
```

Before opening the page, make sure the GLB tile server is running, for example:

```bash
docker compose up --build tileserver-gl osm_pbf_processor
```

Containerized wasm/browser:

```bash
docker compose up --build spacetimedb osm_pbf_processor harbor-wasm
```

The `harbor-wasm` container can inject browser runtime settings from environment
variables in `compose.yml` or `.env`:

- `HARBOR_SPACETIMEDB_URI`
- `HARBOR_SPACETIMEDB_MODULE`
- `HARBOR_SPACETIMEDB_TOKEN`
- `HARBOR_TILE_SERVER_URI`

Then open:

```text
http://127.0.0.1:1334/?spacetimedb_uri=http://localhost:3000&spacetimedb_module=ship-spacetime&tile_server_uri=http://localhost:8081
```

The wasm build reads runtime settings from the browser URL query string:

- `spacetimedb_uri`
- `spacetimedb_module`
- `spacetimedb_token`
- `tile_server_uri`

When both are present, URL query parameters take precedence over the injected
container runtime config.

Example:

```text
http://127.0.0.1:1334/?spacetimedb_uri=http://localhost:3000&spacetimedb_module=ship-spacetime&tile_server_uri=http://localhost:8081
```
