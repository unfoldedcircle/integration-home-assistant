// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Handle response messages from Remote Two

use crate::controller::R2ResponseMsg;
use crate::errors::ServiceError;
use crate::server::ws::WsConn;
use crate::Controller;
use actix::Addr;
use log::{debug, error, warn};
use std::str::FromStr;
use uc_api::intg::ws::R2Response;
use uc_api::ws::WsMessage;

impl WsConn {
    /// Handle response messages from R2
    pub(crate) async fn on_response(
        session_id: &str,
        response: WsMessage,
        controller_addr: Addr<Controller>,
    ) -> Result<(), ServiceError> {
        let msg = response
            .msg
            .as_deref()
            .ok_or_else(|| ServiceError::BadRequest("Missing property: msg".into()))?;

        debug!("[{session_id}] Got response: {msg}");

        if let Ok(resp_msg) = R2Response::from_str(msg) {
            if let Err(e) = controller_addr.try_send(R2ResponseMsg {
                ws_id: session_id.into(),
                msg: resp_msg,
                response,
            }) {
                // avoid returning an Err which would be sent back to the client
                error!("[{session_id}] Controller mailbox error: {e}");
            }
        } else {
            warn!("[{session_id}] Unknown response: {msg}");
        }

        Ok(())
    }
}
