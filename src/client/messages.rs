// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix Actor message definitions for HomeAssistantClient

use crate::client::model::{AssistPipelineEvent, GetPipelinesResult};
use crate::client::translations::TranslationKey;
use crate::errors::ServiceError;
use actix::prelude::Message;
use awc::ws::CloseCode;
use derive_more::Constructor;
use std::collections::{HashMap, HashSet};
use uc_api::intg::ws::{
    BrowseMediaMsgData, BrowseMediaResponseMsgData, SearchMediaMsgData, SearchMediaResponseMsgData,
};
use uc_api::intg::{AvailableIntgEntity, EntityChange, EntityCommand};

/// Call a service in Home Assistant
#[derive(Message)]
#[rtype(result = "Result<(), ServiceError>")]
pub struct CallService {
    /// Remote Two `msg_data` json object from `entity_command` message.
    pub command: EntityCommand,
}

/// Run an assist pipeline in Home Assistant and wait for the response.
///
/// See <https://developers.home-assistant.io/docs/voice/pipelines/>
///
/// Returns:
/// - ServiceError::ServiceUnavailable if the pipeline failed to start
/// - ServiceError::NotFound if the pipeline wasn't found
#[derive(Message)]
#[rtype(result = "Result<(), ServiceError>")]
pub struct CallRunAssistPipeline {
    pub entity_id: String,
    pub session_id: u32,
    pub sample_rate: u32,
    pub timeout: Option<u16>,
    pub speech_response: bool,
    pub pipeline_id: Option<String>,
}

/// Retrieve the display information of the entities from the Home Assistant entity registry.
///
/// See <https://github.com/home-assistant/core/pull/87787>
#[derive(Message, Default)]
#[rtype(result = "Result<EntityRegistryDisplay, ServiceError>")]
pub(super) struct CallListEntityRegistryDisplay;

/// Display information of the Home Assistant entity registry.
#[derive(Debug, Default)]
pub(super) struct EntityRegistryDisplay {
    /// Map: HA entity id -> display precision. Only sensors with a display precision are
    /// included: the user setting in Home Assistant, otherwise the suggested precision of the
    /// sensor.
    pub precisions: HashMap<String, u32>,
    /// Map: HA entity id -> integration and translation key. Only sensors with a translation key
    /// are included.
    pub translation_keys: HashMap<String, TranslationKey>,
}

/// Retrieve the sensor state translations of a translation category from Home Assistant, in the
/// language set with [SetLanguage].
///
/// Returns a map of HA translation resource key -> translated text.
#[derive(Message)]
#[rtype(result = "Result<HashMap<String, String>, ServiceError>")]
pub(super) struct CallGetTranslations {
    /// HA translation category: `entity` or `entity_component`.
    pub category: &'static str,
    /// Only return the translations of these HA integrations.
    pub integrations: Vec<String>,
}

/// Set the language of the Remote, used for the sensor state translations.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SetLanguage {
    /// Language code of the Remote, e.g. `de_CH`.
    pub language: String,
}

/// Retrieve all available assist pipelines from Home Assistant.
#[derive(Message)]
#[rtype(result = "Result<GetPipelinesResult, ServiceError>")]
pub struct CallListAssistPipelines {
    /// Speech to text is required. Pipelines without STT are filtered out.
    pub stt_required: bool,
}

impl Default for CallListAssistPipelines {
    fn default() -> Self {
        Self { stt_required: true }
    }
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

/// HA assist pipeline events
#[derive(Constructor, Message)]
#[rtype(result = "()")]
pub struct AssistEvent {
    /// Remote audio session ID
    pub session_id: u32,
    /// Remote voice assistant entity ID
    pub entity_id: String,
    pub event: AssistPipelineEvent,
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

/// Browse media containers.
#[derive(Message, serde::Deserialize)]
#[rtype(result = "Result<BrowseMediaResponseMsgData, ServiceError>")]
pub struct BrowseMedia(pub BrowseMediaMsgData);

/// Search for media items.
#[derive(Message, serde::Deserialize)]
#[rtype(result = "Result<SearchMediaResponseMsgData, ServiceError>")]
pub struct SearchMedia(pub SearchMediaMsgData);

/// TODO(media-browsing) Get the current playback queue.
#[allow(unused)]
#[derive(Message, serde::Deserialize)]
#[rtype(result = "Result<(), ServiceError>")]
pub struct GetMediaQueue;
