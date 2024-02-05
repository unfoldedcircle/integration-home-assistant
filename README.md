[![Rust](https://github.com/unfoldedcircle/integration-home-assistant/actions/workflows/build.yml/badge.svg)](https://github.com/unfoldedcircle/integration-home-assistant/actions/workflows/build.yml)

# Home-Assistant Integration for Remote Two

This service application connects [Home Assistant](https://www.home-assistant.io/) with the
[Remote Two](https://www.unfoldedcircle.com/) and allows to interact with most entities on the remote.  

The integration is included in the Remote Two firmware and no external service must be run to connect Home Assistant
with Remote Two. The standalone service can be used for development or connecting multiple Home Assistant servers.

The integration implements the Remote Two [Integration-API](https://github.com/unfoldedcircle/core-api) which
communicates with JSON messages over WebSocket.

The WebSocket server and client uses [Actix Web](https://actix.rs/) with the Actix actor system for internal service
communication.


## Container Image

See [Docker image](./docker/README.md) for more information.

## Configuration

- Optional read-only configuration file to override defaults: [configuration.yaml](configuration.yaml)
- User provided Home Integration server settings are written to `$UC_CONFIG_HOME/$UC_USER_CFG_FILENAME`
  once the driver setup flow is run.

Configuration file handling uses the Rust Crate [config](https://docs.rs/config/latest/config/#) which allows
loading configuration values from multiple sources and overwrite default values.

The configuration values can be overwritten with ENV variables.

- Keys containing `_` cannot be overridden. E.g. `websocket.heartbeat.interval_sec`.
- ENV prefix: `UC_`
  - Example: `integration.interface` configuration setting: `UC_INTEGRATION_INTERFACE=127.0.0.1`

### Environment Variables

The following environment variables exist in addition to the configuration file:

| Variable                     | Values               | Description                                                                                                 |
|------------------------------|----------------------|-------------------------------------------------------------------------------------------------------------|
| UC_CONFIG_HOME               | _directory path_     | Configuration directory to save the user configuration from the driver setup.<br>Default: current directory |
| UC_DISABLE_MDNS_PUBLISH      | `true` / `false`     | Disables mDNS service advertisement.<br>Default: `false`                                                    |
| UC_USER_CFG_FILENAME         | _filename_           | JSON configuration filename for the user settings.<br>Default: `home-assistant.json`                        |
| UC_DISABLE_CERT_VERIFICATION | `true` / `false`     | Disables certificate verification for the Home Assistant WS connection.<br>Default: `false`                 |
| UC_API_MSG_TRACING           | `all` / `in` / `out` | Enables incoming and outgoing WS Core-API message tracing<br>Default: no tracing                            |
| UC_HASS_MSG_TRACING          | `all` / `in` / `out` | Enables incoming and outgoing Home Assistant WS message tracing<br>Default: no tracing                      |

On the Remote Two device, the integration is configured for the embedded runtime environment with several environment
variables. Mainly `UC_DISABLE_MDNS_PUBLISH=true`, `UC_CONFIG_HOME` and some `UC_INTEGRATION_*` to listen on the local
interface only.

## How to Build and Run

If you don't have Rust installed yet: <https://www.rust-lang.org/tools/install>

### Build

Without mDNS advertisement support:

```shell
cargo build
```

With [zeroconf](https://crates.io/crates/zeroconf) library, wrapping underlying ZeroConf/mDNS implementations such as
Bonjour on macOS or Avahi on Linux:
```shell
cargo build --features zeroconf
```

With pure Rust [mdns-sd](https://crates.io/crates/mdns-sd) library:

⚠️ Use at own risk, this library can cause package flooding with Apple devices in the same network!

```shell
cargo build --features mdns-sd
```
This will run on any platform, but with limited functionality (no IPv6 support) and potential incompatibilities.

### Run

To start the integration driver:
```shell
cargo run
```

## Home Assistant WebSocket API test tool

The [bin/ha_test.rs](src/bin/ha_test.rs) tool is a simple CLI tool to test the Home Assistant WebSocket API connectivity
with the exact same code & logic as the HA integration for Remote Two.

The main purpose of this tool is to troubleshoot connectivity issues from a PC. Linux, macOS and Windows x86 binaries
are automatically built with a GitHub action and attached to [GitHub releases](https://github.com/unfoldedcircle/integration-home-assistant/releases).

### Functionality

1. Connect to the HA server and authenticate with the token
2. Subscribe to entity state events: `subscribe_events`
3. Request the entity states: `get_states`
4. Disconnect

The debug log including HA message communication is printed to the console.

### Required parameters

- HA long-lived access token.
- HA WebSocket URL. Default if not provided: `ws://homeassistant.local:8123/api/websocket`

The configuration can either be provided in a `home-assistant.json` file in the current directory
(see [./resources/home-assistant.json](resources/home-assistant.json) template), or through command line parameters.


### Usage

```
./ha-test --help
Home Assistant server communication test

Usage: ha-test [OPTIONS]

Options:
  -u <url>                     Home Assistant WebSocket API URL (overrides home-assistant.json)
                               [default: ws://homeassistant.local:8123/api/websocket]
  -t <token>                   Home Assistant long lived access token (overrides home-assistant.json)
  -c <connection_timeout>      TCP connection timeout in seconds (overrides home-assistant.json)
  -r <request_timeout>         Request timeout in seconds (overrides home-assistant.json)
      --trace <MESSAGES>       Message tracing for HA server communication
                               [default: all] [possible values: in, out, all, none]
  -h, --help                   Print help
  -V, --version                Print version
```

## Contributing

Please read [CONTRIBUTING.md](CONTRIBUTING.md) for details on how to contribute and submit pull requests to us.

## Versioning

We use [SemVer](http://semver.org/) for versioning. For the versions available, see the
[tags and releases on this repository](https://github.com/unfoldedcircle/integration-home-assistant/releases).

The major changes found in each new release are listed in the [changelog](./CHANGELOG.md) and
under the GitHub [releases](https://github.com/unfoldedcircle/integration-home-assistant/releases).

## License

This project is licensed under the [**Mozilla Public License 2.0**](https://choosealicense.com/licenses/mpl-2.0/).
See the [LICENSE](LICENSE) file for details.

### Project dependencies

A license report of the projects dependencies can be generated with the
[cargo-about](https://crates.io/crates/cargo-about) tool:

```shell
cargo install cargo-about
cargo about generate about.hbs > integration-hass_licenses.html
```
