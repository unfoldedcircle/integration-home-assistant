// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use serde_json::{Map, Value};

/// Copy (and clone) an entry from one serde_json::Map to another.
///
/// Returns true if an entry has been copied, false if the key could not be found.
pub fn copy_entry(source: &Map<String, Value>, dest: &mut Map<String, Value>, key: &str) -> bool {
    source
        .get(key)
        .map(|v| {
            dest.insert(key.to_string(), v.clone());
        })
        .is_some()
}

/// Move an entry from one serde_json::Map to another without any conversions.
///
/// Returns true if an entry has been moved, false if the key could not be found.
pub fn move_entry(
    source: &mut Map<String, Value>,
    dest: &mut Map<String, Value>,
    key: &str,
) -> bool {
    source
        .remove_entry(key)
        .map(|(k, v)| {
            dest.insert(k, v);
        })
        .is_some()
}

/// Move a value from one serde_json::Map to another while renaming the key.
///
/// Returns true if an entry has been moved, false if the key could not be found.
pub fn move_value(
    source: &mut Map<String, Value>,
    dest: &mut Map<String, Value>,
    key: &str,
    dest_key: impl Into<String>,
) -> bool {
    source
        .remove_entry(key)
        .map(|(_, value)| {
            dest.insert(dest_key.into(), value);
        })
        .is_some()
}

#[allow(dead_code)]
pub fn map_str_value<F: FnOnce(&str) -> Value>(
    source: &Map<String, Value>,
    dest: &mut Map<String, Value>,
    key: &str,
    f: F,
) -> bool {
    source
        .get(key)
        .and_then(|v| v.as_str())
        .map(|v| {
            let v = f(v);
            dest.insert(key.to_string(), v);
        })
        .is_some()
}

pub fn is_float_value(json: &Map<String, Value>, key: &str) -> bool {
    json.get(key).and_then(|v| v.as_f64()).is_some()
}

pub fn number_value(json: &Map<String, Value>, key: &str) -> Option<Value> {
    match json.get(key) {
        Some(v) if v.is_number() => Some(v.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::util::json::{copy_entry, move_entry, move_value};
    use serde_json::{Map, json};

    #[test]
    fn copy_entry_with_non_existing_key_returns_false() {
        let source = Map::new();
        let mut dest = Map::new();
        assert!(
            !copy_entry(&source, &mut dest, "foo"),
            "Non existing key must return false"
        );
    }

    #[test]
    fn copy_entry_with_existing_key_returns_true() {
        let mut source = Map::new();
        let mut dest = Map::new();
        source.insert("foo".into(), "bar".into());
        assert!(
            copy_entry(&source, &mut dest, "foo"),
            "Existing key must return true"
        );
        assert_eq!(Some(&json!("bar")), dest.get("foo"));
    }

    #[test]
    fn move_entry_with_non_existing_key_returns_false() {
        let mut source = Map::new();
        let mut dest = Map::new();
        assert!(
            !move_entry(&mut source, &mut dest, "foo"),
            "Non existing key must return false"
        );
    }

    #[test]
    fn move_entry_with_existing_key_returns_true() {
        let mut source = Map::new();
        let mut dest = Map::new();
        source.insert("foo".into(), "bar".into());
        assert!(
            move_entry(&mut source, &mut dest, "foo"),
            "Existing key must return true"
        );
        assert_eq!(None, source.get("foo"), "Source entry must be removed");
        assert_eq!(Some(&json!("bar")), dest.get("foo"));
    }

    #[test]
    fn move_value_with_non_existing_key_returns_false() {
        let mut source = Map::new();
        let mut dest = Map::new();
        assert!(
            !move_value(&mut source, &mut dest, "foo", "bar"),
            "Non existing key must return false"
        );
    }

    #[test]
    fn move_value_with_existing_key_returns_true() {
        let mut source = Map::new();
        let mut dest = Map::new();
        source.insert("foo".into(), "bar".into());
        assert!(
            move_value(&mut source, &mut dest, "foo", "bar"),
            "Existing key must return true"
        );
        assert_eq!(None, source.get("foo"), "Source entry must be removed");
        assert_eq!(Some(&json!("bar")), dest.get("bar"));
    }
}
