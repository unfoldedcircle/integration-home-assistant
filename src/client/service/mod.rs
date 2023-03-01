// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home Assistant WebSocket service call handler.
//! Translates Remote Two's entity commands into HA `call_service` JSON messages.
//!
//! See <https://developers.home-assistant.io/docs/api/websocket/#calling-a-service> for further
//! information.

use crate::client::messages::CallService;
use crate::client::model::{CallServiceMsg, Target};
use crate::client::HomeAssistantClient;
use crate::errors::ServiceError;
use actix::Handler;
use log::info;
use serde_json::{Map, Value};
use uc_api::intg::EntityCommand;
use uc_api::EntityType;

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
        let (service, service_data) = match msg.command.entity_type {
            EntityType::Button => Ok(("press".to_string(), None)),
            EntityType::Switch => switch::handle_switch(&msg.command),
            EntityType::Climate => climate::handle_climate(&msg.command),
            EntityType::Cover => cover::handle_cover(&msg.command),
            EntityType::Light => light::handle_light(&msg.command),
            EntityType::MediaPlayer => media_player::handle_media_player(&msg.command),
            EntityType::Sensor => Err(ServiceError::BadRequest(
                "Sensor doesn't support sending commands to! Ignoring call".to_string(),
            )),
            EntityType::Activity | EntityType::Macro | EntityType::Remote => {
                Err(ServiceError::BadRequest(format!(
                    "{} is an internal remote-core entity",
                    msg.command.entity_type
                )))
            }
        }?;
        let domain = match msg.command.entity_id.split_once('.') {
            None => return Err(ServiceError::BadRequest("Invalid entity_id format".into())),
            Some((l, _)) => l.to_string(),
        };

        let call_srv_msg = CallServiceMsg {
            id: self.new_msg_id(),
            msg_type: "call_service".to_string(),
            domain,
            service,
            service_data,
            target: Target {
                entity_id: msg.command.entity_id,
            },
        };

        let msg = serde_json::to_value(call_srv_msg)?;
        self.send_json(msg, ctx)

        // TODO wait for HA response message? If the service call fails we'll get a result back with "success: false"
        // However, some services take a long time to respond! E.g. Sonos might take 10 seconds if there's an issue with the network.
    }
}

pub fn cmd_from_str<T: std::str::FromStr + strum::VariantNames>(
    cmd: &str,
) -> Result<T, ServiceError> {
    T::from_str(cmd).map_err(|_| {
        ServiceError::BadRequest(format!(
            "Invalid cmd_id: {cmd}. Valid commands: {}",
            T::VARIANTS.to_vec().join(",")
        ))
    })
}

/// Get a serde_json::Map reference of the params attribute of the provided EntityCommand.
///
/// A BadRequest error is returned if `params` is not set.
fn get_required_params(cmd: &EntityCommand) -> Result<&Map<String, Value>, ServiceError> {
    if let Some(params) = cmd.params.as_ref() {
        Ok(params)
    } else {
        Err(ServiceError::BadRequest("Missing params object".into()))
    }
}
