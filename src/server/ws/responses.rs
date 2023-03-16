// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Handle response messages from Remote Two

use crate::errors::ServiceError;
use crate::server::ws::WsConn;
use crate::Controller;
use actix::Addr;
use log::{info, warn};
use uc_api::ws::WsMessage;

impl WsConn {
    /// Handle response messages from R2
    pub(crate) async fn on_response(
        session_id: &str,
        response: WsMessage,
        _controller_addr: Addr<Controller>,
    ) -> Result<(), ServiceError> {
        let msg = response
            .msg
            .as_deref()
            .ok_or_else(|| ServiceError::BadRequest("Missing property: msg".into()))?;

        match msg {
            "version" => {
                info!("[{session_id}] TODO Handle version response!");
            }
            "supported_entity_types" => {
                info!("[{session_id}] TODO Handle supported_entity_types response!");
            }
            "configured_entities" => {
                info!("[{session_id}] TODO Handle configured_entities response!");
            }
            "localization_cfg" => {
                info!("[{session_id}] TODO Handle localization_cfg response!");
            }
            "setup_user_action" => {
                info!("[{session_id}] TODO Handle setup_user_action message!");
            }
            _ => {
                warn!("[{session_id}] Unknown response: {msg}");
            }
        }

        Ok(())
    }
}
