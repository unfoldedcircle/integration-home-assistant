// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use crate::messages::{R2Event, R2EventMsg};
use crate::server::ws::{api_messages, WsConn};

use actix_web_actors::ws::WebsocketContext;
use log::{error, warn};
use std::str::FromStr;

impl WsConn {
    /// Handle events from R2
    pub(crate) fn on_event(
        &mut self,
        event: api_messages::WsMessage,
        _ctx: &mut WebsocketContext<WsConn>,
    ) {
        let msg = match event.msg {
            None => {
                error!("[{}] Missing msg attribute in event: {:?}", self.id, event);
                return;
            }
            Some(ref m) => m.as_str(),
        };

        if let Ok(req_msg) = R2Event::from_str(msg) {
            if let Err(e) = self.controller_addr.try_send(R2EventMsg {
                ws_id: self.id.clone(),
                event: req_msg,
                msg_data: event.msg_data,
            }) {
                error!("[{}] Controller mailbox error: {}", self.id, e);
            }
        } else {
            warn!("[{}] Unknown event: {}", self.id, msg);
        }
    }
}
