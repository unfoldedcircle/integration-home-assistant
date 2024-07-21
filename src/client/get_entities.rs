// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix actor handler implementation for the `GetStates` message

use actix::Handler;
use log::{debug};
use serde_json::{json};
use crate::client::messages::{GetAvailableEntities};
use crate::client::HomeAssistantClient;
use crate::errors::ServiceError;

impl Handler<GetAvailableEntities> for HomeAssistantClient {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, msg: GetAvailableEntities, ctx: &mut Self::Context) -> Self::Result {
        debug!("[{}] GetAvailableEntities from {}", self.id, msg.remote_id);
        self.remote_id = msg.remote_id;
        let id = self.new_msg_id();

        self.entity_states_id = Some(id);
        // Try to subsscribe again to custom events if not already done when
        // GetAvailableEntities command is received from the remote
        self.send_uc_info_command(ctx);
        self.send_json(
            json!(
            {"id": id, "type": "get_states"}
        ),
            ctx,
        )
    }
}
