# osm_pbf_processor

Converts Mapbox Vector Tile `.pbf` data into GLB scene output.

## Modes

File mode:

```bash
cargo run --bin osm_pbf_processor -- path/to/tile.pbf
```

This writes `path/to/tile.glb` by default.

URL mode:

```bash
cargo run --bin osm_pbf_processor -- --url http://localhost:8080/data 8396 5421 --zoom 14
```

This fetches a tile from the backend and writes a GLB for each requested tile.

Server mode:

```bash
cargo run --bin osm_pbf_processor -- --server
```

This starts an HTTP server that listens on `0.0.0.0:8081` by default and serves routes like:

```text
/data/<zoom>/<x>/<y>.glb
```

For example:

```text
http://localhost:8081/data/14/8396/5421.glb
```

That request is forwarded to the configured backend as:

```text
http://localhost:8080/data/v3/14/8396/5421.pbf
```

Server mode converts tiles in memory and returns the GLB bytes directly. It does not write output files.

## Docker Compose

Start the tileserver and GLB proxy together with:

```bash
docker compose up --build tileserver-gl osm_pbf_processor
```

Then request GLB tiles from:

```text
http://localhost:8081/data/13/4214/2702.glb
```

The Compose service uses `osm_pbf_processor/osm_pbf_processor.compose.toml`, which points the backend at:

```text
http://tileserver-gl:8080
```

This keeps the checked-in local config using `http://localhost:8080` for non-container runs.

To run `location-source` in Compose, provide both placeholders and start it with SpacetimeDB:

```bash
AISSTREAM_API_KEY=replace-with-your-aisstream-api-key \
SPACETIMEDB_DB_NAME=replace-with-your-spacetimedb-db-name \
docker compose up --build spacetimedb location-source
```

Inside Compose, `location-source` connects to SpacetimeDB at:

```text
http://spacetimedb:3000
```

## Config

Configuration is read from `osm_pbf_processor.toml`.

Example:

```toml
[conversion]
output = "harbor/assets/tiles/"
# output_glb = "single.glb"

[server]
bind = "0.0.0.0"
port = 8081
backend = "http://localhost:8080"
```

`conversion.output` is used as the prefix for URL tile exports, producing:

```text
<output>/<zoom>/<x>_<y>.glb
```

For example:

```text
harbor/assets/tiles/14/8396_5421.glb
```

`conversion.output_glb` remains a single-file override for one-off exports and cannot be used with URL x/y ranges.
