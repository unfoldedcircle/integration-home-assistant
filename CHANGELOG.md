# Home-Assistant Integration for Remote Two/3 Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

_Changes in the next release_

### Fixed
- Voice assistant "response_speech" attribute.

---

## v0.14.0 - 2025-11-29

### Added
- Assist support with the new voice_assistant entity ([#78](https://github.com/unfoldedcircle/integration-home-assistant/pull/78)).

### Changed
- Remove the 'v'-prefix from the version information.
- Replace deprecated actix-web-actors with actix-ws ([#77](https://github.com/unfoldedcircle/integration-home-assistant/pull/77)).

## v0.13.1 - 2025-09-11
### Fixed
- Docker image build with updated GitHub actions.

## v0.13.0 - 2025-09-11
### Added
- Expert option to disable certificate validation of the Home Assistant WebSocket server connection ([#71](https://github.com/unfoldedcircle/integration-home-assistant/pull/71)).

### Fixed
- Initial setup with a wss:// Home Assistant URL or switching from ws to wss doesn't require a reboot anymore ([#73](https://github.com/unfoldedcircle/integration-home-assistant/pull/73)).
- Report the correct integration version without the "-dirty" suffix if it was built with a clean codebase.
- Only report the documented sensor states `ON`, `UNAVAILABLE` and `UNKNOWN`.

### Breaking changes
- The mapping of binary sensors has changed to better represent them on the user interface ([#74](https://github.com/unfoldedcircle/integration-home-assistant/pull/74)):
    - The device class is now set to `binary` instead of `custom`.
    - The Home Assistant device class is stored in the `unit` attribute.
    - The `value` attribute is no longer a boolean, but contains the `on` and `off` sensor text values from Home Assistant.

### Changed
- Rustls upgrade to 0.23 with system certificate verifier ([#73](https://github.com/unfoldedcircle/integration-home-assistant/pull/73)).
- Update Rust crates and cross-compile toolchain ([#73](https://github.com/unfoldedcircle/integration-home-assistant/pull/73)).

## v0.12.2 - 2025-04-17
### Added
- Propagate media player attribute `media_position_updated_at` ([feature-and-bug-tracker#443](https://github.com/unfoldedcircle/feature-and-bug-tracker/issues/443)).

### Fixed
- Media player `media_type` attribute value should be upper case to match entity documentation.
### Changed
- update README and driver description ([#67](https://github.com/unfoldedcircle/integration-home-assistant/pull/67)).

## v0.12.1 - 2025-02-17
### Fixed
- Docker image build regression in v0.12.0 ([#65](https://github.com/unfoldedcircle/integration-home-assistant/pull/65)).

## v0.12.0 - 2024-12-13
### Added
- Available entities mode with HA component: get states will retrieve only available entities from HA component instead of all entities in HA. Contributed by @albaintor, thanks! ([#62](https://github.com/unfoldedcircle/integration-home-assistant/pull/62)).

## v0.11.0 - 2024-09-27
### Added
- Automatic configuration and setup through the [Unfolded Circle for Home Assistant component](https://github.com/JackJPowell/hass-unfoldedcircle). In cooperation with @albaintor and @JackJPowell, thanks! ([#60](https://github.com/unfoldedcircle/integration-home-assistant/pull/60)).
  - Please note that the autoconfiguration feature in the Home Assistant component is still under development at the time of this release.
### Fixed
- Avoid initial failed Home Assistant login attempt with default HA server url and empty access token.

## v0.10.0 - 2024-08-24
### Added
- Initial support for the [Unfolded Circle for Home Assistant component](https://github.com/JackJPowell/hass-unfoldedcircle) for optimized message communication. Contributed by @albaintor, thanks! ([#58](https://github.com/unfoldedcircle/integration-home-assistant/pull/58))

### Changed
- Update uc_api crate to latest 0.12.0 version.

## v0.9.0 - 2024-04-10
### Added
- Remote-entity support ([#23](https://github.com/unfoldedcircle/integration-home-assistant/issues/23)).

## v0.8.2 - 2024-03-04
### Changed
- Update uc-api to 0.9.3 for new media-player features (currently not used for Home Assistant).
- Update Rust crates.

## v0.8.1 - 2024-02-27
### Fixed
- driver_version response field.

## v0.8.0 - 2024-02-16
### Added
- Option to disconnect from HA when device enters standby ([#50](https://github.com/unfoldedcircle/integration-home-assistant/issues/50)).
### Changed
- Update Rust crates.

## v0.7.0 - 2024-02-05
### Added
- Home Assistant WebSocket API connection test tool.
### Fixed
- Extract and convert color information from received HA light entities to follow external color changes. Supported color models: xy, hs, rgb ([#7](https://github.com/unfoldedcircle/integration-home-assistant/issues/7)).
- Connection timeout setting was used as request timeout. TCP connection timeout was always set to 5 seconds ([#47](https://github.com/unfoldedcircle/integration-home-assistant/issues/47)).
- Connection state handling in initial setup to avoid restart ([#43](https://github.com/unfoldedcircle/integration-home-assistant/issues/43)).
### Changed
- Immediately close HA WS connection in case of a protocol error.

## v0.6.1 - 2024-01-04
### Fixed
- Reconnection logic regression introduced in v0.6.0

## v0.6.0 - 2024-01-03
### Fixed
- Reconnect to HA server after driver reconfiguration ([#36](https://github.com/unfoldedcircle/integration-home-assistant/issues/36)).
- Improved reconnection logic to prevent multiple connections.

### Changed
- Use Ping-Pong API messages as defined in the HA WebSocket API by default instead of WebSocket ping frames.

## v0.5.1 - 2023-12-17
### Fixed
- Allow unlimited reconnection ([#35](https://github.com/unfoldedcircle/integration-home-assistant/issues/35)).

## v0.5.0 - 2023-11-15
### Added
- Map scenes to push buttons ([#29](https://github.com/unfoldedcircle/integration-home-assistant/issues/29)).

### Changed
- Rename media-player `select_sound_mode` command parameter ([feature-and-bug-tracker#165](https://github.com/unfoldedcircle/feature-and-bug-tracker/issues/165)).
- Update dependencies, including rustls 0.21.

## v0.4.0 - 2023-09-13
### Added
- Allow to use HA Scripts as Button Entity.

## v0.3.0 - 2023-07-17
### Added
- option to use zeroconf library for mDNS advertisement instead of mdns-sd
- new media player features:
  - Add support for input source and sound mode selection.
  - Propagate entity states `standby` and `buffering`.

## v0.2.1 - 2023-05-25
### Fixed
- mdns-sd workaround for mDNS query flooding

## v0.2.0 - 2023-03-28
### Added
- mDNS announcement and `get_driver_metadata` message implementation.
- driver setup flow with main & advanced configuration settings.
- initial TLS WebSocket client and server support.
