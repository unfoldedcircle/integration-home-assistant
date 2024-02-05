# Resources

## driver.json

- Integration driver metadata returned in `get_driver_metadata`.
- Embedded in Rust: [src/main.rs](../src/main.rs)
- `version` property value is overwritten at runtime with application version.
- `token` value is removed if set.
- `driver_id` and `name` are automatically set if missing.

## home-assistant.json

Template configuration file for the [src/bin/ha_test.rs](../src/bin/ha_test.rs) tool.

This is the same configuration file format written by the integration during the setup flow.
