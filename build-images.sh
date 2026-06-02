#!/bin/sh
set -eu

PLATFORMS="${PLATFORMS:-linux/amd64}"

docker buildx inspect >/dev/null 2>&1 || {
    printf '%s'
    exit 1
}

build_and_push() {
    image="$1"
    dockerfile="$2"

    docker buildx build \
        --platform "$PLATFORMS" \
        --push \
        -t "$image" \
        -f "$dockerfile" \
        .
}

#build_and_push "flyaruu/harbor_osm_pbf_processor:latest" "osm_pbf_processor/Dockerfile"
#build_and_push "flyaruu/harbor_location_source:latest" "location-source/Dockerfile"
build_and_push "flyaruu/harbor_harbor_wasm:latest" "harbor/Dockerfile"
