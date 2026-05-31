docker run -it --rm -v $(pwd)/..:/data  ghcr.io/systemed/tilemaker:master /data/map_data/netherlands-latest.osm.pbf  --output /data/map_data/current.pmtiles --config /data/scripts/config.json --process /data/scripts/process-coastline.lua

