// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home Assistant WebSocket event message handling.
//!
//! See <https://developers.home-assistant.io/docs/api/websocket/#subscribe-to-events> for further
//! information.

use crate::client::entity::*;
use crate::client::messages::EntityEvent;
use crate::client::model::Event;
use crate::client::HomeAssistantClient;
use crate::errors::ServiceError;
use log::debug;

impl HomeAssistantClient {
    /// Whenever an `event` message is received from HA, this method is called to handle it.  
    /// The event conversion is delegated to entity type specific functions for the supported entity
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
    pub(crate) fn handle_event(&mut self, event: Event) -> Result<(), ServiceError> {
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
            "button" | "input_button" => {
                // the button entity is stateless and the remote doesn't need to be notified when the button was pressed externally
                return Ok(());
            }
            "cover" => cover_event_to_entity_change(event.data),
            "sensor" => sensor_event_to_entity_change(event.data),
            "binary_sensor" => binary_sensor_event_to_entity_change(event.data),
            "climate" => climate_event_to_entity_change(event.data),
            "media_player" => media_player_event_to_entity_change(&self.server, event.data),
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
