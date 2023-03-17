[![Rust](https://github.com/aitatoi/integration-home-assistant/actions/workflows/build.yml/badge.svg)](https://github.com/aitatoi/integration-home-assistant/actions/workflows/build.yml)

# Home-Assistant Integration for Remote Two

TODO
- introduction
- developer setup
- build
- how to use

## Container Image

See [Docker image](./docker/README.md) for more information.

### Configuration

- Configuration file: [`configuration.yaml`](configuration.yaml)

Configuration file handling uses the Rust Crate [config](https://docs.rs/config/latest/config/#) which allows
loading configuration values from multiple sources and overwrite default values.

The configuration file is for read-only settings! It should be used for system settings which might be tweaked and
optimized. As general rule: the service should run without a configuration file.

The configuration values can be overwritten with ENV variables.

- Keys containing `_` cannot be overridden. E.g. `websocket.heartbeat.interval_sec`.
- ENV prefix: `UC_`
  - Example: `integration.interface` configuration setting: `UC_INTEGRATION_INTERFACE`

## Environment Variables

The following additional environment variables exist to configure additional behaviour:

| Variable                      | Values         | Description                                                            |
|-------------------------------|----------------|------------------------------------------------------------------------|
| UC_DISABLE_CERT_VERIFICATION  | true / false   | Disables certificate verification for the Home Assistant WS connection |
| UC_API_MSG_TRACING            | all / in / out | Enables incoming and outgoing WS Core-API message tracing              |
| UC_HASS_MSG_TRACING           | all / in / out | Enables incoming and outgoing Home Assistant WS message tracing        |

## Contributing

TODO
- CONTRIBUTING.md - example: <https://github.com/regexident/missing_mpl/blob/master/CONTRIBUTING.md>
  - Fork the repo
  - Pass all tests and linting:
    - `cargo test`
    - `cargo clippy`
    - `cargo fmt --all -- --check`
  - Must be licensed under the Mozilla Public License 2.0 (MPL-2.0).  
    It is required to add a boilerplate copyright notice to the top of each file:
    TODO decide on license header! SPDX vs official MPL-2.0 header

    ```
    // Copyright {year} {person OR org} <{email}>
    // SPDX-License-Identifier: MPL-2.0
    ```
    
  - Pull request
- code of conduct?

## Versioning

We use [SemVer](http://semver.org/) for versioning. For the versions available, see the [tags and releases on this repository].

## License

This project is licensed under the [**Mozilla Public License 2.0**](https://choosealicense.com/licenses/mpl-2.0/).
See the [LICENSE](LICENSE) file for details.

### Project dependencies

A license report of the projects dependencies can be generated with the
[cargo-about](https://crates.io/crates/cargo-about) tool:

```shell
cargo install cargo-about
cargo about generate about-markdown.hbs > integration-hass_licenses.md
```
