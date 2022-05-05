// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use actix::Addr;
use std::str::FromStr;

use crate::errors::ServiceError;
use crate::Controller;
use log::{error, warn};
use uc_api::intg::ws::R2Event;
use uc_api::ws::WsMessage;

use crate::messages::R2EventMsg;
use crate::server::ws::WsConn;

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
                error!("[{}] Controller mailbox error: {}", session_id, e);
            }
        } else {
            warn!("[{}] Unknown event: {}", session_id, msg);
        }

        Ok(())
    }
}
