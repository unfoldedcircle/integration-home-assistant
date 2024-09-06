// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix actor handler implementation for the `GetStates` message

use crate::client::messages::SetRemoteId;
use crate::client::HomeAssistantClient;
use crate::errors::ServiceError;
use actix::Handler;
use log::debug;

impl Handler<SetRemoteId> for HomeAssistantClient {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, msg: SetRemoteId, ctx: &mut Self::Context) -> Self::Result {
        debug!("[{}] SetRemoteId from {}", self.id, msg.remote_id);
        self.remote_id = msg.remote_id;
        if self.uc_ha_component {
            self.unsubscribe_uc_configuration(ctx);
            self.subscribe_uc_configuration(ctx);
        }
        Ok(())
    }
}
