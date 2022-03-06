// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! HA WebSocket data structure definitions for JSON serialization & deserialization.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub(crate) struct CallServiceMsg {
    pub id: u32,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub domain: String,
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_data: Option<serde_json::Value>,
    pub target: Target,
}

#[derive(Debug, Serialize)]
pub(crate) struct Target {
    pub entity_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Event {
    //pub event_type: String,
    pub data: EventData,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventData {
    pub entity_id: String,
    pub new_state: EventState,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventState {
    pub state: String,
    pub attributes: Option<serde_json::Map<String, serde_json::Value>>,
}
