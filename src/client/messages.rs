// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix Actor message definitions for HomeAssistantClient

use crate::errors::ServiceError;
use crate::server::{AvailableEntity, EntityChange, EntityCommand};

use actix::prelude::Message;
use awc::ws::CloseCode;

/*
TODO controller -> HA
    x close
    x callService (= sendCommand)
    getStates (= getAvailableEntities)
    subscribeEvent?

TODO HA -> controller
    x connectionState
    x updateEntity
 */
/// Call a service in Home Assistant
#[derive(Message)]
#[rtype(result = "Result<(), ServiceError>")]
pub struct CallService {
    /// Remote Two `msg_data` json object from `entity_command` message.
    pub command: EntityCommand,
}

/// Fetch all states from Home Assistant
#[derive(Message)]
#[rtype(result = "Result<(), ServiceError>")]
pub struct GetStates;

/// Asynchronous response from `GetStates`
#[derive(Message)]
#[rtype(result = "()")]
pub struct AvailableEntities {
    pub client_id: String,
    pub entities: Vec<AvailableEntity>,
}

/// HA client connection states
pub enum ConnectionState {
    AuthenticationFailed,
    Connected,
    Closed,
}

/// HA client connection events
#[derive(Message)]
#[rtype(result = "()")]
pub struct ConnectionEvent {
    pub client_id: String,
    pub state: ConnectionState,
}

/// HA entity events
#[derive(Message)]
#[rtype(result = "()")]
pub struct EntityEvent {
    pub client_id: String,
    pub entity_change: EntityChange,
}

/// HA client request: disconnect and close the session.
// Used internally by the client and from Controller
#[derive(Message)]
#[rtype(result = "()")]
pub struct Close {
    /// WebSocket close code
    pub code: CloseCode,
    pub description: Option<String>,
}

impl Default for Close {
    fn default() -> Self {
        Self {
            code: CloseCode::Normal,
            description: None,
        }
    }
}

impl Close {
    pub fn invalid() -> Self {
        Self {
            code: CloseCode::Invalid,
            description: None,
        }
    }
    pub fn unsupported() -> Self {
        Self {
            code: CloseCode::Unsupported,
            description: None,
        }
    }
}
