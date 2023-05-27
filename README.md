[![Rust](https://github.com/unfoldedcircle/integration-home-assistant/actions/workflows/build.yml/badge.svg)](https://github.com/unfoldedcircle/integration-home-assistant/actions/workflows/build.yml)

# Home-Assistant Integration for Remote Two

This service application connects [Home Assistant](https://www.home-assistant.io/) with the
[Remote Two](https://www.unfoldedcircle.com/) and allows to interact with most entities on the remote.  
It implements the Remote Two [Integration-API](https://github.com/unfoldedcircle/core-api) which communicates with
JSON messages over WebSocket.

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

`cargo build` to build.

`cargo run` to start the integration driver.

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
