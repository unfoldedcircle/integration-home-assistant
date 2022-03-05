// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use crate::server::ws::{api_messages, WsConn};

use actix_web_actors::ws::WebsocketContext;
use log::{error, info, warn};

impl WsConn {
    /// Handle response messages from R2
    pub(crate) fn on_response(
        &mut self,
        response: api_messages::WsMessage,
        _ctx: &mut WebsocketContext<WsConn>,
    ) {
        let msg = match response.msg {
            None => {
                error!(
                    "[{}] Missing msg attribute in response: {:?}",
                    self.id, response
                );
                return;
            }
            Some(ref m) => m.as_str(),
        };

        match msg {
            "version" => {
                info!("[{}] TODO Handle version response!", self.id);
            }
            "supported_entity_types" => {
                info!("[{}] TODO Handle supported_entity_types response!", self.id);
            }
            "configured_entities" => {
                info!("[{}] TODO Handle configured_entities response!", self.id);
            }
            "localization_cfg" => {
                info!("[{}] TODO Handle localization_cfg response!", self.id);
            }
            "setup_user_action" => {
                info!("[{}] TODO Handle setup_user_action message!", self.id);
            }
            _ => {
                warn!("[{}] Unknown response: {}", self.id, msg);
            }
        }
    }
}
