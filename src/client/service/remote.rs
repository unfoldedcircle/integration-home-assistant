// Copyright (c) 2024 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Remote entity specific HA service call logic.

use crate::client::service::{cmd_from_str, get_required_params};
use crate::errors::ServiceError;
use serde_json::{Map, Value};
use uc_api::intg::EntityCommand;
use uc_api::RemoteCommand;

pub(crate) fn handle_remote(msg: &EntityCommand) -> Result<(String, Option<Value>), ServiceError> {
    let cmd: RemoteCommand = cmd_from_str(&msg.cmd_id)?;

    let result = match cmd {
        RemoteCommand::On => ("turn_on".into(), None),
        RemoteCommand::Off => ("turn_off".into(), None),
        RemoteCommand::Toggle => ("toggle".into(), None),
        RemoteCommand::Send => create_command(msg, "command")?,
        RemoteCommand::SendSequence => create_command(msg, "sequence")?,
        RemoteCommand::StopSend => return Err(ServiceError::NotYetImplemented),
    };

    Ok(result)
}

fn create_command(msg: &EntityCommand, cmd: &str) -> Result<(String, Option<Value>), ServiceError> {
    let mut data = Map::new();
    let params = get_required_params(msg)?;
    if let Some(value) = params.get(cmd) {
        if cmd == "sequence" && value.is_array() || cmd == "command" && value.is_string() {
            data.insert("command".into(), value.clone());
        }
    }
    if data.is_empty() {
        return Err(ServiceError::BadRequest(format!(
            "Invalid or missing attribute: params.{}",
            cmd
        )));
    }
    if let Some(value) = params.get("repeat").and_then(|v| v.as_u64()) {
        data.insert("num_repeats".into(), value.into());
    }
    if let Some(value) = params
        .get("delay")
        .and_then(|v| v.as_u64())
        .map(|v| v as f32 / 1000f32)
    {
        data.insert("delay_secs".into(), value.into());
    }
    if let Some(value) = params
        .get("hold")
        .and_then(|v| v.as_u64())
        .map(|v| v as f32 / 1000f32)
    {
        data.insert("hold_secs".into(), value.into());
    }
    Ok(("send_command".into(), Some(data.into())))
}
