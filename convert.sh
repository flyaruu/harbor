docker run -it --rm --pull always -v $(pwd):/data \
  ghcr.io/systemed/tilemaker:master \
  /data/data/netherlands-260519.osm.pbf \
  --output /data/current.pmtiles
cargo run --release -p osm_pbf_processor -- --url http://localhost:8080/data 8368-8400 5412-5421 --zoom 14 --output harbor/assets/tiles

