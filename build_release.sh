#!/bin/bash

ARCH=$(uname -m)
IMAGE="cosmwasm/workspace-optimizer:0.14.0"

if [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ]; then
    IMAGE="cosmwasm/workspace-optimizer-arm64:0.14.0"
fi

docker run --rm -v "$(pwd)":/code \
    --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
    --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
    $IMAGE
