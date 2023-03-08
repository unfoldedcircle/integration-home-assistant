# Resources

## driver.json

- Integration driver metadata returned in `get_driver_metadata`.
- Embedded in Rust: [src/main.rs](../src/main.rs)
- `version` property value is overwritten at runtime with application version.
- `token` value is removed if set.
- `driver_id` and `name` are automatically set if missing.
