# Container Image for Home-Assistant Integration

The provided [Dockerfile](Dockerfile) creates a Linux amd64 container for the Home Assistant integration.

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

To run the Home Assistant integration you need the Home Assistant server API URL and a long-lived access token.

They can either be provided by the user in the driver setup flow, or statically configured in the configuration file or
set with the environment variables `UC_HASS_URL` and `UC_HASS_TOKEN`.

By default, the integration tries to connect to <ws://homeassistant.local:8123/api/websocket>.

Provide configuration with the driver setup flow and store user configuration in a volume:
```bash
docker run --rm --name uc-intg-hass \
  -p 8000:8000 \
  integration-hass:latest
```

The configuration will be saved in the `$UC_CONFIG_HOME` directory, which is by default a volume. This can also be
bind-mounted to the host (directory needs to be writeable for user_id 10000):
```bash
docker run --rm --name uc-intg-hass \
  -p 8000:8000 \
  -v $YOUR_HOST_CONFIG_DIRECTORY:/config \
  integration-hass:latest
```

The Home Assistant server configuration can also be set with environment variables:
```bash
docker run --rm --name uc-intg-hass \
  -e UC_HASS_URL=$YOUR_HOME_ASSISTANT_URL \
  -e UC_HASS_TOKEN=$YOUR_LONG_LIVED_ACCESS_TOKEN \
  -p 8000:8000 \
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
