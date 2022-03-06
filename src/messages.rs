// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use actix::prelude::{Message, Recipient};
use serde::Deserialize;
use strum_macros::{EnumMessage, EnumString};

use uc_api::ws::WsMessage;
use uc_api::DeviceState;

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendWsMessage(pub WsMessage);

#[derive(Message)]
#[rtype(result = "Result<(), std::io::Error>")]
pub struct Connect {
    // device identifier not required: only single HA connection supported
// pub device_id: String,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    // device identifier not required: only single HA connection supported
// pub device_id: String,
}

/// New WebSocket connection from R2 established
#[derive(Message)]
#[rtype(result = "()")]
pub struct NewR2Session {
    /// Actor address of the WS session to send messages to
    pub addr: Recipient<SendWsMessage>,
    /// unique identifier of WS connection
    pub id: String,
}

/// R2 WebSocket disconnected
#[derive(Message)]
#[rtype(result = "()")]
pub struct R2SessionDisconnect {
    /// unique identifier of WS connection
    pub id: String,
}

#[derive(Message)]
#[rtype(result = "DeviceState")]
pub struct GetDeviceState {
    // device identifier not required: only single HA connection supported
// pub device_id: String,
}

#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct R2RequestMsg {
    pub ws_id: String,
    pub req_id: u32,
    pub request: R2Request,
    pub msg_data: Option<serde_json::Value>,
}

/// Remote Two initiated request messages.
/// The corresponding response message name is set with the strum message macro
#[derive(Debug, Deserialize, EnumMessage, EnumString)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum R2Request {
    #[strum(message = "driver_version")]
    GetDriverVersion,
    #[strum(message = "device_state")]
    GetDeviceState,
    #[strum(message = "device_setup_complete")]
    SetupDevice,
    #[strum(message = "available_entities")]
    GetAvailableEntities,
    #[strum(message = "result")]
    SubscribeEvents,
    #[strum(message = "result")]
    UnsubscribeEvents,
    #[strum(message = "entity_states")]
    GetEntityStates,
    #[strum(message = "result")]
    EntityCommand,
}

#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct R2EventMsg {
    pub ws_id: String,
    pub event: R2Event,
    pub msg_data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, EnumMessage, EnumString)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum R2Event {
    Connect,
    Disconnect,
    EnterStandby,
    ExitStandby,
    StartDiscovery,
    StopDiscovery,
    AbortDeviceSetup,
}

#[derive(Debug, Deserialize, EnumMessage, EnumString)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum R2Response {
    Version,
    SupportedEntityTypes,
    ConfiguredEntities,
    LocalizationCfg,
    SetupUserAction,
}
