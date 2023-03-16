// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Handle events from Remote Two

use crate::errors::ServiceError;
use crate::messages::R2EventMsg;
use crate::server::ws::WsConn;
use crate::Controller;
use actix::Addr;
use log::{error, warn};
use std::str::FromStr;
use uc_api::intg::ws::R2Event;
use uc_api::ws::WsMessage;

impl WsConn {
    /// Handle events from R2
    pub(crate) async fn on_event(
        session_id: &str,
        event: WsMessage,
        controller_addr: Addr<Controller>,
    ) -> Result<(), ServiceError> {
        let msg = event
            .msg
            .as_deref()
            .ok_or_else(|| ServiceError::BadRequest("Missing property: msg".into()))?;

        if let Ok(req_msg) = R2Event::from_str(msg) {
            if let Err(e) = controller_addr.try_send(R2EventMsg {
                ws_id: session_id.into(),
                event: req_msg,
                msg_data: event.msg_data,
            }) {
                error!("[{session_id}] Controller mailbox error: {e}");
            }
        } else {
            warn!("[{session_id}] Unknown event: {msg}");
        }

        Ok(())
    }
}
