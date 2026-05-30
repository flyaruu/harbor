#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset

./get-landcover.sh
./get-netherlands.sh
./get-coastline.sh