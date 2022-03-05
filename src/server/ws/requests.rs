// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use crate::messages::R2Request;
use crate::messages::R2RequestMsg;
use crate::server::ws::{api_messages, WsConn};

use actix_web_actors::ws::WebsocketContext;
use log::{error, warn};
use std::str::FromStr;

impl WsConn {
    /// Handle request messages from R2
    pub(crate) fn on_request(
        &mut self,
        request: api_messages::WsMessage,
        ctx: &mut WebsocketContext<WsConn>,
    ) {
        let id = match request.id {
            None => {
                self.send_missing_field_error(0, "id", ctx);
                return;
            }
            Some(id) => id,
        };
        let msg = match request.msg {
            None => {
                self.send_missing_field_error(id, "msg", ctx);
                return;
            }
            Some(ref m) => m.as_str(),
        };

        if let Ok(req_msg) = R2Request::from_str(msg) {
            if let Err(e) = self.controller_addr.try_send(R2RequestMsg {
                ws_id: self.id.clone(),
                req_id: id,
                request: req_msg,
                msg_data: request.msg_data,
            }) {
                error!("[{}] Controller mailbox error: {}", self.id, e);
                self.send_error(
                    id,
                    500,
                    "INTERNAL_ERROR",
                    "Error processing request".into(),
                    ctx,
                );
            }
        } else {
            warn!("[{}] Unknown message: {}", self.id, msg);
            self.send_error(
                id,
                400,
                "BAD_REQUEST",
                format!("Unknown message: {}", msg),
                ctx,
            );
        }
    }
}
