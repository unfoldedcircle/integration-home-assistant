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
        self.sessions.insert(msg.id.clone(), R2Session::new(msg.addr));

        self.send_device_state(&msg.id);

        // Retrieve the runtime info to store the remote id (used later to identify the remote
        // from unified HA component
        // TODO @Markus store the runtime_info.remote_identifier but no remote ID in the runtime info
        //  Example of what is transmitted :
        //  {"driver_id": "hass", "intg_ids": ["hass.main"], extra: {} }
        if let Some(session) = self.sessions.get_mut(&msg.id) {
            // Avoid conflict with next received request ids
            // let mut requestid: i32 = (&msg.id).parse().unwrap();
            // requestid += 100;
            let requestid = 32768;
            //TODO @Markus this message type should be an EVENT and not a REQUEST :
            // WS connection R2 => HA driver : R2 = server, HA driver = client
            // Meantime I have built the message from a json object which is not clean
            // With event type we will be able to remove the fakeid
            let json = serde_json::json!({
                        "kind": "req",
                        "id": Some(requestid),
                        "msg": "get_runtime_info",
                    });
            let request: WsMessage =
                serde_json::from_value(json).expect("Invalid json message");

            match session.recipient.try_send(SendWsMessage(request)) {
                Ok(_) => info!("[{}] Request sent", requestid),
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
