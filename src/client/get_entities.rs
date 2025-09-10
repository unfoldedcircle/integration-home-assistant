// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix actor handler implementation for the `GetStates` message

use crate::client::HomeAssistantClient;
use crate::client::messages::GetAvailableEntities;
use crate::errors::ServiceError;
use actix::Handler;
use log::debug;
use serde_json::json;

impl Handler<GetAvailableEntities> for HomeAssistantClient {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, msg: GetAvailableEntities, ctx: &mut Self::Context) -> Self::Result {
        debug!("[{}] GetAvailableEntities from {}", self.id, msg.remote_id);
        self.remote_id = msg.remote_id;
        let id = self.new_msg_id();

        self.entity_states_id = Some(id);
        // Try to subscribe again to custom events if not already done when
        // GetAvailableEntities command is received from the remote
        self.send_uc_info_command(ctx);
        if self.uc_ha_component {
            // Retrieve the states of available entities (including subscribed entities)
            // Available entities are defined on HA component side and should include
            // subscribed entities but sent anyway just in case some are missing
            debug!(
                "[{}] Get states from {} with unfoldedcircle/get_states",
                self.id, self.remote_id
            );
            self.send_json(
                json!(
                    {"id": id, "type": "unfoldedcircle/entities/states",
                    "data": {
                        "entity_ids": self.subscribed_entities,
                        "client_id": self.remote_id
                    }}
                ),
                ctx,
            )
        } else {
            debug!("[{}] Get standard states from {} ", self.id, self.remote_id);
            self.send_json(
                json!(
                    {"id": id, "type": "get_states"}
                ),
                ctx,
            )
        }
    }
}
