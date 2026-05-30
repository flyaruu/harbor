#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset

mkdir -p "../map_data/"
pushd "../map_data/"

if ! [ -f "netherlands-latest.osm.pbf" ]; then
  curl -fO https://download.geofabrik.de/europe/netherlands-latest.osm.pbf
fi

popd
