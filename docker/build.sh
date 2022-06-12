#!/bin/bash

set -o errexit
set -o pipefail

if [ "$(uname)" == "Darwin" ]; then
  echo "This build script is only for Linux"
  exit 1
fi

cargo build --release
mkdir -p ./app
cp ../target/release/uc-intg-hass ./app/
cp ../configuration.yaml ./app/

BUILD_LABELS="\
--build-arg BUILD_DATE=$(date -u +"%Y-%m-%dT%H:%M:%SZ") \
--build-arg VERSION=$(git describe --match "v[0-9]*" --tags HEAD --always) \
--build-arg REVISION=$(git log -1 --format="%H")"

docker build $BUILD_LABELS -t integration-hass .
