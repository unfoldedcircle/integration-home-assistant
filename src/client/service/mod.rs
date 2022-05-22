// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home Assistant WebSocket service call handler.
//! Translates Remote Two's entity commands into HA `call_service` JSON messages.
//!
//! See <https://developers.home-assistant.io/docs/api/websocket/#calling-a-service> for further
//! information.

use actix::Handler;
use actix_web_actors::ws;
use log::info;

use uc_api::EntityType;

use crate::client::messages::CallService;
use crate::client::model::{CallServiceMsg, Target};
use crate::client::HomeAssistantClient;
use crate::errors::ServiceError;

mod climate;
mod cover;
mod light;
mod media_player;
mod switch;

impl Handler<CallService> for HomeAssistantClient {
    type Result = Result<(), ServiceError>;

    /// Convert a R2 `EntityCommand` to a HA `call_service` request and send it as WebSocket text
    /// message.  
    /// The conversion of the entity logic is delegated to entity specific functions in this crate.
    ///
    /// # Arguments
    ///
    /// * `msg`: Actor message containing the R2 `EntityCommand` structure.
    /// * `ctx`: Actor execution context
    ///
    /// returns: Result<(), ServiceError>
    fn handle(&mut self, msg: CallService, ctx: &mut Self::Context) -> Self::Result {
        info!("[{}] Calling service in HomeAssistant", self.id);

        // map Remote Two command name & parameters to HA service name and service_data payload
        let ha = match msg.command.entity_type {
            EntityType::Button => Ok(("press".to_string(), None)),
            EntityType::Switch => switch::handle_switch(&msg),
            EntityType::Climate => climate::handle_climate(&msg),
            EntityType::Cover => cover::handle_cover(&msg),
            EntityType::Light => light::handle_light(&msg),
            EntityType::MediaPlayer => media_player::handle_media_player(&msg),
            EntityType::Sensor => Err(ServiceError::BadRequest(
                "Sensor doesn't support sending commands to! Ignoring call".to_string(),
            )),
        }?;

        let call_srv_msg = CallServiceMsg {
            id: self.new_msg_id(),
            msg_type: "call_service".to_string(),
            domain: msg.command.entity_type.to_string(), // only works since we use the same name as Home Assistant :-)
            service: ha.0,
            service_data: ha.1,
            target: Target {
                entity_id: msg.command.entity_id,
            },
        };

        let msg = serde_json::to_string(&call_srv_msg).map(|v| ws::Message::Text(v.into()))?;
        self.send_message(msg, "call_service", ctx)

        // TODO wait for HA response message? If the service call fails we'll get a result back with "success: false"
        // However, some services take a long time to respond! E.g. Sonos might take 10 seconds if there's an issue with the network.
    }
}

pub fn cmd_from_str<T: std::str::FromStr + strum::VariantNames>(
    cmd: &str,
) -> Result<T, ServiceError> {
    T::from_str(cmd).map_err(|_| {
        ServiceError::BadRequest(format!(
            "Invalid cmd_id: {}. Valid commands: {}",
            cmd,
            T::VARIANTS.to_vec().join(",")
        ))
    })
}
