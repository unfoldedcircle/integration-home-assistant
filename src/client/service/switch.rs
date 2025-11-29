// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Switch entity specific HA service call logic.

use crate::client::cmd_from_str;
use crate::errors::ServiceError;
use serde_json::Value;
use uc_api::SwitchCommand;
use uc_api::intg::EntityCommand;

pub(crate) fn handle_switch(msg: &EntityCommand) -> Result<(String, Option<Value>), ServiceError> {
    let cmd: SwitchCommand = cmd_from_str(&msg.cmd_id)?;

    let result = match cmd {
        SwitchCommand::On => ("turn_on".to_string(), None),
        SwitchCommand::Off => ("turn_off".to_string(), None),
        SwitchCommand::Toggle => ("Toggle".to_string(), None),
    };

    Ok(result)
}
