// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use serde_json::{Map, Value};

/// Copy (and clone) an entry from one serde_json::Map to another.
pub fn copy_entry(source: &Map<String, Value>, dest: &mut Map<String, Value>, key: &str) -> bool {
    source
        .get(key)
        .map(|v| {
            dest.insert(key.to_string(), v.clone());
        })
        .is_some()
}

#[cfg(test)]
mod tests {
    use crate::util::json::copy_entry;
    use serde_json::{json, Map};

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
}
