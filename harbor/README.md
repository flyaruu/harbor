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

The wasm build reads SpacetimeDB connection settings from the browser URL query string:

- `spacetimedb_uri`
- `spacetimedb_module`
- `spacetimedb_token`

Example:

```text
http://127.0.0.1:1334/?spacetimedb_uri=http://localhost:3000&spacetimedb_module=ship-spacetime
```
