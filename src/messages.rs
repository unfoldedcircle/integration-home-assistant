// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix actor message definitions used to communicate with the Controller from the R2 server
//! connections and the Home Assistant client connections.

use crate::errors::ServiceError;
use crate::from_msg_data::DeserializeMsgData;
use actix::prelude::{Message, Recipient};
use uc_api::ws::intg::{R2Event, R2Request};
use uc_api::ws::WsMessage;
use uc_api::DeviceState;

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendWsMessage(pub WsMessage);

/// Connect to Home Assistant
#[derive(Message)]
#[rtype(result = "Result<(), std::io::Error>")]
pub struct Connect {
    // TODO device identifier for multi-HA connections: not yet implemented
// pub device_id: String,
}

/// Disconnect from Home Assistant
#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    // TODO device identifier for multi-HA connections: not yet implemented
// pub device_id: String,
}

/// Internal message to delegate R2Request::SubscribeEvents request
#[derive(Debug, Message)]
#[rtype(result = "Result<(), ServiceError>")]
pub struct SubscribeHassEvents(pub R2RequestMsg);

/// Internal message to delegate R2Request::UnsubscribeEvents request
#[derive(Debug, Message)]
#[rtype(result = "Result<(), ServiceError>")]
pub struct UnsubscribeHassEvents(pub R2RequestMsg);

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

/// Actor message for a Remote Two request.
#[derive(Debug, Message)]
#[rtype(result = "Result<(), ServiceError>")]
pub struct R2RequestMsg {
    pub ws_id: String,
    pub req_id: u32,
    pub request: R2Request,
    pub msg_data: Option<serde_json::Value>,
}

/// Convert the full request message to only the message data payload.
///
/// Required for DeserializeMsgData trait.
#[allow(clippy::from_over_into)] // we only need into
impl Into<Option<serde_json::Value>> for R2RequestMsg {
    fn into(self) -> Option<serde_json::Value> {
        self.msg_data
    }
}

impl DeserializeMsgData for R2RequestMsg {}

/// Actor message for a Remote Two event.
#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct R2EventMsg {
    pub ws_id: String,
    pub event: R2Event,
    pub msg_data: Option<serde_json::Value>,
}
