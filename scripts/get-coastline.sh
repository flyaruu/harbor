#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset


mkdir -p "../map_data/coastline"
pushd "../map_data/coastline"

if ! [ -f "water-polygons-split-4326.zip" ]; then
  curl -fO https://osmdata.openstreetmap.de/download/water-polygons-split-4326.zip
else
  echo "File water-polygons-split-4326.zip already exists, skipping download."
fi

unzip -o -j water-polygons-split-4326.zip

popd
