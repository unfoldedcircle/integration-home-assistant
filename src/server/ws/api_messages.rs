// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use actix::prelude::Message;
use log::error;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use serde_with::skip_serializing_none;
use std::collections::HashMap;
use std::time::SystemTime;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// generic message definition for requests, responses and events
#[derive(Message)]
#[rtype(result = "()")]
#[skip_serializing_none]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct WsMessage {
    pub kind: Option<String>,
    pub id: Option<u32>,
    pub req_id: Option<u32>,
    pub msg: Option<String>,
    pub code: Option<u16>,
    pub cat: Option<EventCategory>,
    pub ts: Option<String>,
    pub msg_data: Option<Value>,
}

fn to_rfc3339<T>(dt: T) -> Option<String>
where
    T: Into<OffsetDateTime>,
{
    dt.into().format(&Rfc3339).ok()
}

impl WsMessage {
    pub fn event(msg: &str, cat: Option<EventCategory>, msg_data: Value) -> Self {
        Self {
            kind: Some("event".into()),
            msg: Some(msg.into()),
            cat,
            ts: to_rfc3339(SystemTime::now()),
            msg_data: Some(msg_data),
            ..Default::default()
        }
    }

    pub fn response_json(req_id: u32, msg: &str, msg_data: Value) -> Self {
        Self {
            kind: Some("resp".into()),
            req_id: Some(req_id),
            msg: Some(msg.into()),
            msg_data: Some(msg_data),
            ..Default::default()
        }
    }

    pub fn response<T: serde::Serialize>(req_id: u32, msg: &str, msg_data: T) -> Self {
        match serde_json::to_value(msg_data) {
            Ok(v) => Self {
                kind: Some("resp".into()),
                req_id: Some(req_id),
                msg: Some(msg.into()),
                code: Some(200),
                msg_data: Some(v),
                ..Default::default()
            },

            Err(e) => {
                error!("Error serializing struct: {:?}", e);
                Self {
                    kind: Some("resp".into()),
                    req_id: Some(req_id),
                    msg: Some("result".into()),
                    code: Some(500),
                    msg_data: Some(
                        json!({ "code": "INTERNAL_ERROR", "message": "Error serializing result"}),
                    ),
                    ..Default::default()
                }
            }
        }
    }

    pub fn error(req_id: u32, code: u16, msg_data: WsError) -> Self {
        Self {
            kind: Some("resp".into()),
            req_id: Some(req_id),
            msg: Some("result".into()),
            code: Some(code),
            msg_data: Some(
                serde_json::to_value(msg_data).expect("Error serializing model::Error struct"),
            ),
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WsRequest {
    pub kind: String,
    pub id: u32,
    pub msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_data: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WsResponse {
    pub kind: String,
    pub req_id: u32,
    pub msg: String,
    pub code: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_data: Option<Value>,
}

impl WsResponse {
    pub fn new<T: serde::Serialize>(req_id: u32, msg: &str, msg_data: T) -> Self {
        // even though our structs should always be able to deserialize, better be safe...
        match serde_json::to_value(msg_data) {
            Ok(v) => Self {
                kind: "resp".into(),
                req_id,
                msg: msg.into(),
                code: 200,
                msg_data: Some(v),
            },
            Err(e) => {
                error!("Error serializing struct: {:?}", e);
                Self {
                    kind: "resp".into(),
                    req_id,
                    msg: "result".into(),
                    code: 500,
                    msg_data: Some(
                        json!({ "code": "INTERNAL_ERROR", "message": "Error serializing result"}),
                    ),
                }
            }
        }
    }

    pub fn error(req_id: u32, code: u16, msg_data: WsError) -> Self {
        Self {
            kind: "resp".into(),
            req_id,
            msg: "result".into(),
            code,
            msg_data: Some(
                serde_json::to_value(msg_data).expect("Error serializing model::Error struct"),
            ),
        }
    }

    pub fn missing_field(req_id: u32, field: &str) -> Self {
        Self {
            kind: "resp".into(),
            req_id,
            msg: "result".into(),
            code: 400,
            msg_data: Some(
                json!({ "code": "BAD_REQUEST", "message": format!("Missing field: {}", field)}),
            ),
        }
    }

    pub fn result(req_id: u32, code: u16) -> Self {
        Self {
            kind: "resp".into(),
            req_id,
            msg: "result".into(),
            code,
            msg_data: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WsError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventCategory {
    Device,
    Entity,
    Remote,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeviceState {
    Connecting,
    Connected,
    Disconnected,
    Error,
}

#[derive(Debug, Serialize)]
pub(crate) struct ApiVersion {
    pub api: String,
    pub integration: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SubscribeEvents {
    pub device_id: Option<String>,
    pub entity_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct EntityCommand {
    pub device_id: Option<String>,
    pub entity_type: EntityType,
    pub entity_id: String,
    pub cmd_id: String,
    pub params: Option<Value>,
}

#[derive(
    Debug, strum_macros::Display, strum_macros::EnumString, PartialEq, Serialize, Deserialize,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Button,
    Switch,
    Climate,
    Cover,
    Light,
    MediaPlayer,
    Sensor,
}

#[derive(strum_macros::EnumString, PartialEq, Deserialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum SwitchCommand {
    On,
    Off,
    Toggle,
}

#[derive(strum_macros::EnumString, PartialEq, Deserialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ClimateCommand {
    On,
    Off,
    HvacMode,
    TargetTemperature,
    // TargetTemperatureRange,
    // FanMode,
}

#[derive(strum_macros::EnumString, PartialEq, Deserialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum CoverCommand {
    Open,
    Close,
    Stop,
    Position,
}

#[derive(strum_macros::EnumString, PartialEq, Deserialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum LightCommand {
    On,
    Off,
    Toggle,
}

#[derive(strum_macros::EnumString, PartialEq, Deserialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MediaPlayerCommand {
    On,
    Off,
    Toggle,
    PlayPause,
    Stop,
    Previous,
    Next,
    FastForward,
    Rewind,
    Seek,
    Volume,
    VolumeUp,
    VolumeDown,
    MuteToggle,
    Mute,
    Unmute,
    Repeat,
    Shuffle,
}

#[derive(Debug, Serialize)]
pub struct EntityChange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    pub entity_type: EntityType,
    pub entity_id: String,
    pub attributes: serde_json::Map<String, Value>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize)]
pub struct AvailableEntity {
    pub device_id: Option<String>,
    pub entity_type: EntityType,
    pub entity_id: String,
    pub device_class: Option<String>,
    pub friendly_name: HashMap<String, String>,
    pub features: Option<Vec<String>>,
    pub area: Option<String>,
    pub options: Option<serde_json::Map<String, Value>>,
}

/// All available Remote Two switch entity device classes as an enum for type safe handling.
/// See API documentation for more information.
#[derive(Debug, strum_macros::Display, Serialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum SwitchDeviceClass {
    Outlet,
    Switch,
}

/// All available Remote Two climate entity features as an enum for type safe handling.
/// See API documentation for more information.
///
/// The enum value will be serialized into the untyped string attribute in the JSON message with the
/// `strum_macros::Display` macro and "snake_case" option.
#[derive(Debug, strum_macros::Display, Serialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ClimateFeature {
    OnOff,
    Heat,
    Cool,
    CurrentTemperature,
    TargetTemperature,
    //TargetTemperatureRange Not yet implemented
    //Fan Not yet implemented
}

/// All available Remote Two climate entity options as an enum for type safe handling.
/// See API documentation for more information.
///
/// The enum value will be serialized into the untyped string attribute in the JSON message with the
/// `strum_macros::Display` macro and "snake_case" option.
#[derive(Debug, strum_macros::Display, Serialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ClimateOption {
    TemperatureUnit,
    TargetTemperatureStep,
    MaxTemperature,
    MinTemperature,
    //FanModes Not yet implemented
}

/// All available Remote Two cover entity features as an enum for type safe handling.
/// See API documentation for more information.
#[derive(Debug, strum_macros::Display, Serialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum CoverFeature {
    Open,
    Close,
    Stop,
    Position,
    // Tilt,
    // TiltStop,
    // TiltPosition,
}

/// All available Remote Two light entity features as an enum for type safe handling.
/// See API documentation for more information.
#[derive(Debug, strum_macros::Display, Serialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum LightFeature {
    OnOff,
    Toggle,
    Dim,
    Color,
    ColorTemperature,
}

/// All available Remote Two media player entity features as an enum for type safe handling.
/// See API documentation for more information.
#[derive(Debug, strum_macros::Display, Serialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MediaPlayerFeature {
    OnOff,
    Toggle,
    Volume,
    VolumeUpDown,
    MuteToggle,
    Mute,
    Unmute,
    PlayPause,
    Stop,
    Next,
    Previous,
    FastForward,
    Rewind,
    Repeat,
    Shuffle,
    Seek,
    MediaDuration,
    MediaPosition,
    MediaTitle,
    MediaArtist,
    MediaAlbum,
    MediaImageUrl,
    MediaType,
}

/// All available Remote Two sensor entity options as an enum for type safe handling.
/// See API documentation for more information.
#[derive(Debug, strum_macros::Display, Serialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum SensorOption {
    /// Label for a custom sensor if `device_class` is not specified or to override a default unit.
    CustomLabel,
    /// Unit label for a custom sensor if `device_class` is not specified or to override a default
    /// unit.
    CustomUnit,
    /// The sensor's native unit of measurement to perform automatic conversion. Applicable to
    /// device classes: `temperature`.
    NativeUnit,
    /// Number of decimal places to show in the UI if the sensor provides the measurement as a
    /// number. Not applicable to string values.
    Decimals,
}
