// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Climate entity specific HA service call logic.

use crate::client::messages::CallService;
use crate::errors::ServiceError;
use crate::server::ClimateCommand;

use serde_json::Value;
use std::str::FromStr;

pub(crate) fn handle_climate(msg: &CallService) -> Result<(String, Option<Value>), ServiceError> {
    let _cmd = ClimateCommand::from_str(&msg.command.cmd_id)?;

    Err(ServiceError::NotYetImplemented)
}
