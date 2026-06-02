#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
INPUT_PBF="$REPO_ROOT/map_data/netherlands-latest.osm.pbf"
OUTPUT_PMTILES="$REPO_ROOT/map_data/current.pmtiles"
CONFIG_JSON="$SCRIPT_DIR/config.json"
PROCESS_LUA="$SCRIPT_DIR/process-coastline.lua"

if [ ! -f "$INPUT_PBF" ]; then
    printf '%s\n' "Missing input PBF: $INPUT_PBF" >&2
    exit 1
fi

if [ ! -f "$CONFIG_JSON" ]; then
    printf '%s\n' "Missing Tilemaker config: $CONFIG_JSON" >&2
    exit 1
fi

if [ ! -f "$PROCESS_LUA" ]; then
    printf '%s\n' "Missing Tilemaker process script: $PROCESS_LUA" >&2
    exit 1
fi

mkdir -p "$REPO_ROOT/map_data"

docker run --rm -it \
    -v "$REPO_ROOT:/data" \
    ghcr.io/systemed/tilemaker:master \
    /data/map_data/netherlands-latest.osm.pbf \
    --output /data/map_data/current.pmtiles \
    --config /data/scripts/config.json \
    --process /data/scripts/process-coastline.lua

printf '%s\n' "Wrote $OUTPUT_PMTILES"
