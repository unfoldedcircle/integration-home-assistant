// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Climate entity specific HA service call logic.

use crate::client::service::cmd_from_str;
use crate::errors::ServiceError;
use serde_json::Value;
use uc_api::intg::EntityCommand;
use uc_api::ClimateCommand;

pub(crate) fn handle_climate(msg: &EntityCommand) -> Result<(String, Option<Value>), ServiceError> {
    let _cmd: ClimateCommand = cmd_from_str(&msg.cmd_id)?;

    Err(ServiceError::NotYetImplemented)
}
