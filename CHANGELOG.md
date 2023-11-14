# Home-Assistant Integration for Remote Two Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

_Changes in the next release_

### Added
- Map scenes to push buttons ([#29](https://github.com/unfoldedcircle/integration-home-assistant/issues/29)).

### Changed
- Rename media-player `select_sound_mode` command parameter

---

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
