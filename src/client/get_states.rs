// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix actor handler implementation for the `GetStates` message

use std::str::FromStr;

use actix::Handler;
use log::{debug, error, warn};
use serde_json::{json, Value};
use uc_api::EntityType;

use crate::client::entity::*;
use crate::client::messages::{AvailableEntities, GetStates};
use crate::client::HomeAssistantClient;
use crate::errors::ServiceError;

impl Handler<GetStates> for HomeAssistantClient {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, _: GetStates, ctx: &mut Self::Context) -> Self::Result {
        debug!("[{}] GetStates", self.id);

        let id = self.new_msg_id();
        self.entity_states_id = Some(id);
        self.send_json(
            json!(
                {"id": id, "type": "get_states"}
            ),
            ctx,
        )
    }
}

impl HomeAssistantClient {
    pub(crate) fn handle_get_states_result(
        &mut self,
        entities: Vec<Value>,
    ) -> Result<(), ServiceError> {
        let mut available = Vec::with_capacity(32);

        for mut entity in entities {
            let entity_id = entity
                .get("entity_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let entity_id = entity_id.to_string();
            let error_id = entity_id.to_string();
            let entity_type = match entity_id.split_once('.') {
                None => {
                    error!(
                        "[{}] Invalid entity_id format, missing dot to extract domain: {entity_id}",
                        self.id
                    );
                    continue; // best effort
                }
                // map different entity type names
                Some((domain, _)) => match domain {
                    "input_boolean" => "switch",
                    "binary_sensor" => "sensor",
                    "input_button" => "button",
                    v => v,
                },
            };

            let entity_type = match EntityType::from_str(entity_type) {
                Err(_) => {
                    debug!("[{}] Filtering non-supported entity: {entity_id}", self.id);
                    continue;
                }
                Ok(v) => v,
            };

            let state = entity
                .get("state")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string())
                .unwrap_or_default();
            let attr = match entity.get_mut("attributes").and_then(|v| v.as_object_mut()) {
                None => {
                    warn!(
                        "[{}] Could not convert HASS entity {error_id}: missing attributes",
                        self.id
                    );
                    continue;
                }
                Some(o) => o,
            };

            let avail_entity = match entity_type {
                EntityType::Button => convert_button_entity(entity_id, state, attr),
                EntityType::Switch => convert_switch_entity(entity_id, state, attr),
                EntityType::Climate => convert_climate_entity(entity_id, state, attr),
                EntityType::Cover => convert_cover_entity(entity_id, state, attr),
                EntityType::Light => convert_light_entity(entity_id, state, attr),
                EntityType::MediaPlayer => {
                    convert_media_player_entity(&self.server, entity_id, state, attr)
                }
                EntityType::Sensor => convert_sensor_entity(entity_id, state, attr),
            };

            match avail_entity {
                Ok(entity) => available.push(entity),
                Err(e) => warn!(
                    "[{}] Could not convert HASS entity {error_id}: {e:?}",
                    self.id
                ),
            }
        }

        self.controller_actor.try_send(AvailableEntities {
            client_id: self.id.clone(),
            entities: available,
        })?;

        Ok(())
    }
}
