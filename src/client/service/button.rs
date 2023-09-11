// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Button entity specific HA service call logic.

use crate::client::service::cmd_from_str;
use crate::errors::ServiceError;
use serde_json::Value;
use uc_api::intg::EntityCommand;
use uc_api::ButtonCommand;

pub(crate) fn handle_button(msg: &EntityCommand) -> Result<(String, Option<Value>), ServiceError> {
    let cmd: ButtonCommand = cmd_from_str(&msg.cmd_id)?;

    let entity: Vec<&str> = msg.entity_id.split('.').collect();

    let service_call: &str = match entity[0] {
        "script" => entity[1],
        &_ => "press",
    };

    let result = match cmd {
        ButtonCommand::Push => (service_call.into(), None),
    };

    Ok(result)
}
