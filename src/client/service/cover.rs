// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Cover entity specific HA service call logic.

use crate::client::messages::CallService;
use crate::client::service::cmd_from_str;
use crate::errors::ServiceError;
use serde_json::{Map, Value};
use uc_api::CoverCommand;

pub(crate) fn handle_cover(msg: &CallService) -> Result<(String, Option<Value>), ServiceError> {
    let cmd: CoverCommand = cmd_from_str(&msg.command.cmd_id)?;

    let result = match cmd {
        CoverCommand::Open => ("open_cover".into(), None),
        CoverCommand::Close => ("close_cover".into(), None),
        CoverCommand::Stop => ("stop_cover".into(), None),
        CoverCommand::Position => {
            let mut data = Map::new();
            if let Some(params) = msg.command.params.as_ref() {
                if let Some(pos @ 0..=100) = params.get("position").and_then(|v| v.as_u64()) {
                    data.insert("position".into(), Value::Number(pos.into()));
                }
            }
            ("set_cover_position".into(), Some(data.into()))
        } // TODO implement tilt command
    };

    Ok(result)
}
