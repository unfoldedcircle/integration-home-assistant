// Copyright (c) 2025 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Select entity specific HA service call logic.

use crate::client::cmd_from_str;
use crate::errors::ServiceError;
use serde_json::Value;
use uc_api::SelectCommand;
use uc_api::intg::EntityCommand;

pub(crate) fn handle_select(msg: &EntityCommand) -> Result<(String, Option<Value>), ServiceError> {
    let cmd: SelectCommand = cmd_from_str(&msg.cmd_id)?;

    let result = match cmd {
        SelectCommand::SelectOption => (
            "select_option".into(),
            msg.params.clone().map(Value::Object),
        ),
        SelectCommand::SelectFirst => ("select_first".into(), None),
        SelectCommand::SelectLast => ("select_last".into(), None),
        SelectCommand::SelectNext => ("select_next".into(), msg.params.clone().map(Value::Object)),
        SelectCommand::SelectPrevious => (
            "select_previous".into(),
            msg.params.clone().map(Value::Object),
        ),
    };

    Ok(result)
}
