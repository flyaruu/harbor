docker run -it --rm -v $(pwd)/..:/data  ghcr.io/systemed/tilemaker:master /data/data/netherlands-260522.osm.pbf  --output /data/map_data/current.pmtiles --config /data/scripts/config.json # --process /data/data/process-coastline.lua

