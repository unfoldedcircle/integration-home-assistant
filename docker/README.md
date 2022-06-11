# Container Image for Home-Assistant Integration

The provided [Dockerfile](Dockerfile) creates a Linux container for the Home Assistant integration.

The information on this page are for building a container image yourself on a Linux host.  
To get the latest image from us, you can simply pull it from Dockerhub:

```bash
docker pull docker.io/unfoldedcircle/integration-hass
```

## Build

Just run the provided `build.sh` script. This builds the project and creates the container image.

Minimal manual build:
```bash
cargo build --release
mkdir -p app
cp ../target/release/uc-intg-hass /app
docker build -t integration-hass .
```

See [build script](build.sh) for more information, e.g. optional configuration file and build labels.

## Run

To run the Home Assistant integration you need to set your access token in the environment variable `UC_HASS_TOKEN`.  
By default the integration tries to connect to <ws://hassio.local:8123/api/websocket>. This can be overridden with the
environment variable `UC_HASS_URL`.

```bash
docker run --rm --name uc-intg-hass \
  -e UC_HASS_URL=$YOUR_HOME_ASSISTANT_URL \
  -e UC_HASS_TOKEN=$YOUR_LONG_LIVED_ACCESS_TOKEN \
  -p 8000:8000 \
  -p 8443:8443 \
  integration-hass:latest
```

## FAQ

- Where do I get the long-lived access token from?
  - A long-lived access token must be created in the Home Assistant user profile (your name, bottom left).
- Why is this in a subdirectory?
  - To reduce the build context. After building debug & release versions the full project can easily reach multiple
    gigabytes instead of just a few megabytes for the static release binary.
- Why just a Linux container?
  - Our main development environment is Linux and our resources are limited.
  - We happily accept PRs to support other architectures.
