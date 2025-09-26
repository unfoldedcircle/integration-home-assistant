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
use uc_api::intg::ws::{R2Event, R2Request, R2Response};
use uc_api::ws::WsMessage;

/// Send a WebSocket message to Remote Two.
///
/// The [`WsMessage`] is either an Integration-API request, response or event message.
/// Sending is best-effort only!
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendWsMessage(pub WsMessage);

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

/// Actor message for a Remote Two request.
///
/// Pass an integration API request message fom a connected integration client to the
/// [`Controller`]. The controller can either respond directly with a response if some [WsMessage]
/// is returned, or asynchronously at a later time if `None` is returned.
///
/// - a returned [ServiceError] will mapped to an error response message for the Remote Two.
#[derive(Debug, Message)]
#[rtype(result = "Result<Option<WsMessage>, ServiceError>")]
pub struct R2RequestMsg {
    pub ws_id: String,
    pub req_id: u32,
    pub request: R2Request,
    pub msg_data: Option<serde_json::Value>,
}

/// Actor message for a Remote Two response.
#[derive(Debug, Message)]
#[rtype(result = "()")]
#[allow(dead_code)] // response not used
pub struct R2ResponseMsg {
    pub ws_id: String,
    pub msg: R2Response,
    pub response: WsMessage,
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
#[allow(dead_code)] // msg_data not used
pub struct R2EventMsg {
    pub ws_id: String,
    pub event: R2Event,
    pub msg_data: Option<serde_json::Value>,
}
