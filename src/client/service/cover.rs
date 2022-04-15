// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Cover entity specific HA service call logic.

use serde_json::Value;
use uc_api::CoverCommand;

use crate::client::messages::CallService;
use crate::client::service::cmd_from_str;
use crate::errors::ServiceError;

pub(crate) fn handle_cover(msg: &CallService) -> Result<(String, Option<Value>), ServiceError> {
    let _cmd: CoverCommand = cmd_from_str(&msg.command.cmd_id)?;

    Err(ServiceError::NotYetImplemented)
}
