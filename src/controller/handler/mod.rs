// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handlers.

mod ha_connection;
mod ha_event;
mod r2_connection;
mod r2_event;
mod r2_request;
mod r2_response;
mod setup;

use crate::controller::R2RequestMsg;
use crate::errors::ServiceError;
use actix::Message;
use uc_api::intg::{IntegrationSetup, SetupDriver};

/// Internal message to delegate [`R2Request::SubscribeEvents`] requests.
#[derive(Debug, Message)]
#[rtype(result = "Result<(), ServiceError>")]
struct SubscribeHaEventsMsg(pub R2RequestMsg);

/// Internal message to delegate [`R2Request::UnsubscribeEvents`] requests.
#[derive(Debug, Message)]
#[rtype(result = "Result<(), ServiceError>")]
struct UnsubscribeHaEventsMsg(pub R2RequestMsg);

/// Internal message to connect to Home Assistant.
#[derive(Message, Default)]
#[rtype(result = "Result<(), std::io::Error>")]
struct ConnectMsg {
    // device identifier for multi-HA connections: feature not yet available
    // pub device_id: String,
}

/// Internal message to disconnect from Home Assistant.
#[derive(Message)]
#[rtype(result = "()")]
struct DisconnectMsg {
    // device identifier for multi-HA connections: feature not yet available
    // pub device_id: String,
}

/// Internal message to start driver setup flow.
#[derive(Message)]
#[rtype(result = "Result<(), ServiceError>")]
struct SetupDriverMsg {
    pub ws_id: String,
    pub data: SetupDriver,
}

/// Internal message to set driver setup input data
#[derive(Message)]
#[rtype(result = "Result<(), ServiceError>")]
struct SetDriverUserDataMsg {
    pub ws_id: String,
    pub data: IntegrationSetup,
}

/// Internal message to abort setup flow due to a timeout or an abort message from Remote Two.
#[derive(Message)]
#[rtype(result = "()")]
pub(crate) struct AbortDriverSetup {
    pub ws_id: String,
    /// internal timeout
    pub timeout: bool,
}
