#!/bin/sh
set -eu

python3 - <<'PY'
import json
import os
from pathlib import Path

mapping = {
    "spacetimedb_uri": os.environ.get("HARBOR_SPACETIMEDB_URI", ""),
    "spacetimedb_module": os.environ.get("HARBOR_SPACETIMEDB_MODULE", ""),
    "spacetimedb_token": os.environ.get("HARBOR_SPACETIMEDB_TOKEN", ""),
    "tile_server_uri": os.environ.get("HARBOR_TILE_SERVER_URI", ""),
}

config = {key: value for key, value in mapping.items() if value}
Path("/srv/harbor/runtime-config.js").write_text(
    "window.__HARBOR_RUNTIME_CONFIG__ = " + json.dumps(config) + ";\n",
    encoding="utf-8",
)
PY

exec python3 -m http.server 1334 --bind 0.0.0.0 --directory /srv/harbor
