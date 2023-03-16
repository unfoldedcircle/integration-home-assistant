// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Handle request messages from Remote Two

use crate::errors::ServiceError;
use crate::messages::R2RequestMsg;
use crate::server::ws::WsConn;
use crate::Controller;
use actix::Addr;
use log::{debug, warn};
use std::str::FromStr;
use uc_api::intg::ws::R2Request;
use uc_api::ws::WsMessage;

impl WsConn {
    /// Handle request messages from R2
    pub(crate) async fn on_request(
        session_id: &str,
        request: WsMessage,
        controller_addr: Addr<Controller>,
    ) -> Result<(), ServiceError> {
        debug!("[{session_id}] Got request: {request:?}");
        let id = request
            .id
            .ok_or_else(|| ServiceError::BadRequest("Missing property: id".into()))?;
        let msg = request
            .msg
            .as_deref()
            .ok_or_else(|| ServiceError::BadRequest("Missing property: msg".into()))?;

        if let Ok(req_msg) = R2Request::from_str(msg) {
            controller_addr
                .send(R2RequestMsg {
                    ws_id: session_id.into(),
                    req_id: id,
                    request: req_msg,
                    msg_data: request.msg_data,
                })
                .await?
        } else {
            warn!("[{session_id}] Unknown message: {msg}");
            Err(ServiceError::BadRequest(format!("Unknown message: {msg}")))
        }
    }
}
