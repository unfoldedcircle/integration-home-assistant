// Copyright (c) 2022-2023 {person OR org} <{email}>
// SPDX-License-Identifier: MPL-2.0

use serde::de::{DeserializeOwned, Error};
use serde_json::Value;

/// Deserialize a serde json value from a generic message to a typed message struct.
#[allow(dead_code)]
pub trait DeserializeMsgData: Into<Option<Value>> {
    fn deserialize<T: DeserializeOwned>(self) -> Result<T, serde_json::Error> {
        match self.into() {
            None => Err(serde_json::Error::custom("Missing field: 'msg_data'")),
            Some(m) => T::deserialize(m),
        }
    }

    fn deserialize_or_default<T: DeserializeOwned + Default>(self) -> Result<T, serde_json::Error> {
        match self.into() {
            None => Ok(T::default()), // optional
            Some(m) => T::deserialize(m),
        }
    }
}
