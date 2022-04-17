// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use actix::Addr;
use log::{info, warn};

use crate::errors::ServiceError;
use crate::Controller;
use uc_api::ws::WsMessage;

use crate::server::ws::WsConn;

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
                info!("[{}] TODO Handle version response!", session_id);
            }
            "supported_entity_types" => {
                info!(
                    "[{}] TODO Handle supported_entity_types response!",
                    session_id
                );
            }
            "configured_entities" => {
                info!("[{}] TODO Handle configured_entities response!", session_id);
            }
            "localization_cfg" => {
                info!("[{}] TODO Handle localization_cfg response!", session_id);
            }
            "setup_user_action" => {
                info!("[{}] TODO Handle setup_user_action message!", session_id);
            }
            _ => {
                warn!("[{}] Unknown response: {}", session_id, msg);
            }
        }

        Ok(())
    }
}
