// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix `Actor` trait implementation.

use actix::{Actor, Context};
use log::debug;

use crate::client::HomeAssistantClient;
use crate::client::messages::{ConnectionEvent, ConnectionState};

impl Actor for HomeAssistantClient {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        debug!("[{}] HA client started", self.id);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        debug!("[{}] HA client stopped", self.id);
        // stop reconnecting and close the connection, if not already done
        self.ws.do_send(actix_ws_client::Disconnect);
        self.controller_actor.do_send(ConnectionEvent {
            client_id: self.id.clone(),
            state: ConnectionState::Closed,
        });
    }
}
