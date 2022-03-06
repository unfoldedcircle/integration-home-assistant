// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix `Actor` trait implementation.

use actix::{Actor, Context};
use log::debug;

use crate::client::messages::{ConnectionEvent, ConnectionState};
use crate::client::HomeAssistantClient;

impl Actor for HomeAssistantClient {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        debug!("[{}] HA client started", self.id);
        self.heartbeat(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        debug!("[{}] HA client stopped", self.id);
        self.controller_actor.do_send(ConnectionEvent {
            client_id: self.id.clone(),
            state: ConnectionState::Closed,
        });
    }
}
