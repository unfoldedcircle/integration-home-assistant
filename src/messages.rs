// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix actor message definitions used to communicate with the [`Controller`].
//!
//! These are the Actix messages used for the Remote Two WebSocket server connections and the
//! Home Assistant client connections to interact with the Controller.

#[allow(unused_imports)] // used for doc links
use crate::controller::Controller;
use crate::errors::ServiceError;
use crate::util::DeserializeMsgData;
use actix::prelude::{Message, Recipient};
use uc_api::intg::ws::{R2Event, R2Request};
use uc_api::intg::DeviceState;
use uc_api::ws::WsMessage;

/// Send a WebSocket message to Remote Two.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendWsMessage(pub WsMessage);

/// Connect to Home Assistant.
#[derive(Message)]
#[rtype(result = "Result<(), std::io::Error>")]
pub struct Connect {
    // TODO device identifier for multi-HA connections: feature not yet available
    // pub device_id: String,
}

/// Disconnect from Home Assistant.
#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    // TODO device identifier for multi-HA connections: feature not yet available
    // pub device_id: String,
}

/// Internal message to delegate [`R2Request::SubscribeEvents`] requests.
#[derive(Debug, Message)]
#[rtype(result = "Result<(), ServiceError>")]
pub struct SubscribeHassEvents(pub R2RequestMsg);

/// Internal message to delegate [`R2Request::UnsubscribeEvents`] requests.
#[derive(Debug, Message)]
#[rtype(result = "Result<(), ServiceError>")]
pub struct UnsubscribeHassEvents(pub R2RequestMsg);

/// New WebSocket connection from Remote Two established.
///
/// Event to notify the [`Controller`] that a new WS integration client connected.
#[derive(Message)]
#[rtype(result = "()")]
pub struct NewR2Session {
    /// Actor address of the WS session to send messages to
    pub addr: Recipient<SendWsMessage>,
    /// unique identifier of WS connection
    pub id: String,
}

/// Remote Two WebSocket connection disconnected.
///
/// Event to notify the [`Controller`] that a WS client connection disconnected.
#[derive(Message)]
#[rtype(result = "()")]
pub struct R2SessionDisconnect {
    /// unique identifier of WS connection
    pub id: String,
}

/// Get the Home Assistant connection device states.
///
/// Returns [`DeviceState`] enum.
#[derive(Message)]
#[rtype(result = "DeviceState")]
pub struct GetDeviceState {
    // device identifier not required: only single HA connection supported
    // pub device_id: String,
}

/// Actor message for a Remote Two request.
///
/// Pass an integration API request message fom a connected integration client to the [`Controller`].
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
/// Required for [`DeserializeMsgData`] trait.
#[allow(clippy::from_over_into)] // we only need into
impl Into<Option<serde_json::Value>> for R2RequestMsg {
    fn into(self) -> Option<serde_json::Value> {
        self.msg_data
    }
}

impl DeserializeMsgData for R2RequestMsg {}

/// Actor message for a Remote Two event.
///
/// Pass an integration API event message fom a connected integration client to the [`Controller`].
#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct R2EventMsg {
    pub ws_id: String,
    pub event: R2Event,
    pub msg_data: Option<serde_json::Value>,
}
