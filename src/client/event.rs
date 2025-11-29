// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home Assistant WebSocket event message handling.
//!
//! See <https://developers.home-assistant.io/docs/api/websocket/#subscribe-to-events> for further
//! information.

use crate::client::HomeAssistantClient;
use crate::client::entity::*;
use crate::client::messages::{AssistEvent, EntityEvent};
use crate::client::model::{AssistPipelineEvent, Event};
use crate::errors::ServiceError;
use log::{debug, warn};
use serde_json::json;
use std::time::Instant;

impl HomeAssistantClient {
    /// Whenever an entity `event` message is received from HA, this method is called to handle it.
    /// The event conversion is delegated to entity-type-specific functions for the supported entity
    /// types.  
    ///
    /// The converted `EntityChange` is sent to the controller in an Actix `EntityEvent` message to
    /// be delegated to the connected remotes.
    ///
    /// # Arguments
    ///
    /// * `event`: Transformed `.event` json object containing only the required data.
    ///
    /// returns: Result<(), ServiceError>
    pub(super) fn handle_entity_event(&mut self, event: Event) -> Result<(), ServiceError> {
        let entity_type = match event.data.entity_id.split_once('.') {
            None => return Err(ServiceError::BadRequest("Invalid entity_id format".into())),
            Some((l, _)) => l,
        };

        if event.data.entity_id.is_empty() || event.data.new_state.state.is_empty() {
            return Err(ServiceError::BadRequest(format!(
                "Missing data in state_changed event: {:?}",
                event.data
            )));
        }

        let entity_change = match entity_type {
            "light" => light_event_to_entity_change(event.data),
            "switch" | "input_boolean" => switch_event_to_entity_change(event.data),
            "button" | "input_button" | "script" => {
                // the button & script entity is stateless and the remote doesn't need to be notified when the button was pressed externally
                return Ok(());
            }
            "cover" => cover_event_to_entity_change(event.data),
            "sensor" | "binary_sensor" => sensor_event_to_entity_change(event.data),
            "climate" => climate_event_to_entity_change(event.data),
            "media_player" => media_player_event_to_entity_change(&self.server, event.data),
            "remote" => remote_event_to_entity_change(event.data),
            &_ => {
                debug!("[{}] Unsupported entity: {}", self.id, entity_type);
                return Ok(()); // it's not really an error, so it's ok ;-)
            }
        }?;

        self.controller_actor.try_send(EntityEvent {
            client_id: self.id.clone(),
            entity_change,
        })?;

        Ok(())
    }

    pub(super) fn handle_assist_pipeline_event(
        &mut self,
        id: u32,
        mut event: AssistPipelineEvent,
    ) -> Result<(), ServiceError> {
        self.remove_expired_assist_sessions();

        let session = match self.assist_sessions.get_mut(&id) {
            None => {
                warn!(
                    "[{}] no assist session found for id {id}: ignoring event {event:?}",
                    self.id
                );
                return Ok(());
            }
            Some(session) => session,
        };

        debug!("[{}] assist pipeline event: {:?}", self.id, event);
        // intercept events to update the session state or patch certain fields
        match &mut event {
            AssistPipelineEvent::RunStart { data } => {
                let bin_id = data.runner_data.as_ref().map(|d| d.stt_binary_handler_id);
                session.stt_binary_handler_id = bin_id;
            }
            AssistPipelineEvent::SttEnd { .. } => {}
            AssistPipelineEvent::IntentEnd { .. } => {}
            AssistPipelineEvent::RunEnd => {
                // Don't remove session yet: we might still get an Error event AFTER RunEnd!
                // Session will be removed in `remove_expired_assist_sessions`.
                session.run_end = Some(Instant::now());
            }
            AssistPipelineEvent::Error { data } => {
                // we might still get a RunEnd event after an Error event!
                session.error = Some(data.clone());
            }
            // not (yet) interested in the remaining events:
            AssistPipelineEvent::WakeWordStart => {}
            AssistPipelineEvent::WakeWordEnd => {}
            AssistPipelineEvent::SttStart { .. } => {}
            AssistPipelineEvent::SttVadStart => {}
            AssistPipelineEvent::SttVadEnd => {}
            AssistPipelineEvent::IntentStart { .. } => {}
            AssistPipelineEvent::IntentProgress => {}
            AssistPipelineEvent::TtsStart { .. } => {}
            AssistPipelineEvent::TtsEnd { data } => {
                if let Some(output) = data.tts_output.as_mut()
                    && output.url.starts_with('/')
                {
                    output.url = format!(
                        "{}://{}:{}{}",
                        self.server.scheme(),
                        self.server.host_str().unwrap_or_default(),
                        self.server.port_or_known_default().unwrap_or_default(),
                        output.url
                    );
                }
            }
        }

        let _ = self.controller_actor.try_send(AssistEvent::new(
            session.session_id,
            session.entity_id.clone(),
            event,
        ));

        Ok(())
    }
}

/// Convert a HA sensor state to a UC sensor-entity state.
///
/// The UC sensor entity only supports the ON state, and the common entity states:
/// https://unfoldedcircle.github.io/core-api/entities/entity_sensor.html#states
/// # Arguments
///
/// * `state`: Home Assistant sensor or binary-sensor state.
///
/// returns: "ON", "UNAVAILABLE", or "UNKNOWN"
pub(crate) fn convert_ha_sensor_state(state: &str) -> Result<serde_json::Value, ServiceError> {
    match state {
        "unavailable" | "unknown" => Ok(serde_json::Value::String(state.to_uppercase())),
        &_ => Ok(json!("ON")),
    }
}

pub(crate) fn convert_ha_onoff_state(state: &str) -> Result<serde_json::Value, ServiceError> {
    match state {
        "on" | "off" | "unavailable" | "unknown" => {
            Ok(serde_json::Value::String(state.to_uppercase()))
        }
        &_ => Err(ServiceError::BadRequest(format!(
            "Unknown state: {}",
            state
        ))),
    }
}
