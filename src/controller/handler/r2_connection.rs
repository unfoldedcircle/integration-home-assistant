// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for Remote Two connection messages.

use crate::controller::handler::r2_connection;
use crate::controller::{Controller, NewR2Session, R2Session, R2SessionDisconnect, SendWsMessage};
use actix::{Context, Handler};
use log::{error, info};
use serde_json::json;
use std::collections::HashMap;
use uc_api::ws::{WsMessage, WsRequest};

// TODO : Markus the UCAPI should be able to convert requests to messages (WsRequest is an impl
//  of WsMessage structure
trait From<WsRequest> {
    fn from(f: WsRequest) -> Self;
}
impl From<WsRequest> for WsMessage {
    fn from(f: WsRequest) -> Self {
        WsMessage {
            kind: Some(f.kind),
            id: Some(f.id),
            msg: Some(f.msg),
            msg_data: f.msg_data,
            req_id: None,
            code: None,
            extra: HashMap::new(),
            ts: None,
            cat: None,
        }
    }
}

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
            if let Ok(request) = WsRequest::new(request_id, "get_version", json!({})) {
                let message = <WsMessage as r2_connection::From<WsRequest>>::from(request);
                match session.recipient.try_send(SendWsMessage(message)) {
                    Ok(_) => info!("[{}] Request sent", request_id),
                    Err(e) => error!("[{}] Error sending entity_states: {e:?}", msg.id),
                }
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
