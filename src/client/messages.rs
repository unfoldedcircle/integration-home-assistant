// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix Actor message definitions for HomeAssistantClient

use actix::prelude::Message;
use awc::ws::CloseCode;
use std::collections::HashSet;

use uc_api::intg::{AvailableIntgEntity, EntityChange, EntityCommand};

use crate::errors::ServiceError;

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
pub struct GetStates {
    pub remote_id: String,
    pub entity_ids: HashSet<String>,
}

/// Get available entities from Home Assistant
#[derive(Message)]
#[rtype(result = "Result<(), ServiceError>")]
pub struct GetAvailableEntities {
    pub remote_id: String,
}

/// Asynchronous HA response from `GetStates`
#[derive(Message)]
#[rtype(result = "()")]
#[allow(dead_code)] // client_id not used
pub struct AvailableEntities {
    pub client_id: String,
    pub entities: Vec<AvailableIntgEntity>,
}

/// Asynchronous HA response from `GetStates`
#[derive(Message)]
#[rtype(result = "()")]
pub struct SetAvailableEntities {
    #[allow(dead_code)]
    pub client_id: String,
    pub entities: Vec<AvailableIntgEntity>,
}

/// Sent by controller when subscribed entities change
/// TODO : identifier necessary for multiple remotes ?
#[derive(Message)]
#[rtype(result = "()")]
pub struct SubscribedEntities {
    pub entity_ids: HashSet<String>,
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
#[allow(dead_code)] // client_id not used
pub struct EntityEvent {
    pub client_id: String,
    pub entity_change: EntityChange,
}

/// Set remote id from remote to client
#[derive(Message)]
#[rtype(result = "Result<(), ServiceError>")]
pub struct SetRemoteId {
    pub remote_id: String,
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
