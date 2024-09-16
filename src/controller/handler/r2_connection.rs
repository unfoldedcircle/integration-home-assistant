// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for Remote Two connection messages.

use crate::controller::{Controller, NewR2Session, R2Session, R2SessionDisconnect, SendWsMessage};
use actix::{Context, Handler};
use log::{error, info};
use uc_api::ws::WsMessage;

impl Handler<NewR2Session> for Controller {
    type Result = ();

    fn handle(&mut self, msg: NewR2Session, _: &mut Context<Self>) -> Self::Result {
        self.sessions
            .insert(msg.id.clone(), R2Session::new(msg.addr));

        self.send_device_state(&msg.id);

        // Retrieve the version info to store the remote id (used later to identify the remote
        // from unified HA component
        if let Some(session) = self.sessions.get_mut(&msg.id) {
            let request_id = session.new_msg_id();
            let message = WsMessage {
                kind: Some("req".into()),
                id: Some(request_id),
                msg: Some("get_version".into()),
                ..Default::default()
            };
            match session.recipient.try_send(SendWsMessage(message)) {
                Ok(_) => info!("[{}] Request sent", request_id),
                Err(e) => error!("[{}] Error sending entity_states: {e:?}", msg.id),
            }
        }
    }
}

impl Handler<R2SessionDisconnect> for Controller {
    type Result = ();

    fn handle(&mut self, msg: R2SessionDisconnect, _: &mut Context<Self>) {
        self.sessions.remove(&msg.id);
    }
}
