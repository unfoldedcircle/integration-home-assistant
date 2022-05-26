// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Climate entity specific HA service call logic.

use crate::client::service::{cmd_from_str, get_required_params};
use crate::errors::ServiceError;
use crate::util::json::copy_entry;
use serde_json::{json, Map, Value};
use uc_api::intg::EntityCommand;
use uc_api::ClimateCommand;

pub(crate) fn handle_climate(msg: &EntityCommand) -> Result<(String, Option<Value>), ServiceError> {
    let cmd: ClimateCommand = cmd_from_str(&msg.cmd_id)?;

    let result = match cmd {
        ClimateCommand::On => ("turn_on".into(), None),
        ClimateCommand::Off => ("turn_off".into(), None),
        ClimateCommand::HvacMode => {
            let mut data = Map::new();
            let params = get_required_params(msg)?;
            let mode = params
                .get("hvac_mode")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            match mode {
                "OFF" | "HEAT" | "COOL" | "HEAT_COOL" | "AUTO" => {
                    data.insert("state".into(), mode.to_lowercase().into());
                }
                "FAN" => {
                    data.insert("state".into(), "fan_only".into());
                }
                _ => {
                    return Err(ServiceError::BadRequest(format!(
                        "Invalid or missing params.hvac_mode attribute: {}",
                        mode
                    )));
                }
            }

            // TODO can we send a temperature param in set_hvac_mode?
            // If not: remove example from entity docs...
            copy_entry(params, &mut data, "temperature");

            ("set_hvac_mode".into(), Some(data.into()))
        }
        ClimateCommand::TargetTemperature => {
            let params = get_required_params(msg)?;
            if let Some(temp) = params.get("temperature").and_then(|v| v.as_f64()) {
                (
                    "set_temperature".into(),
                    Some(json!({ "temperature": temp })),
                )
            } else {
                return Err(ServiceError::BadRequest(
                    "Invalid or missing params.temperature attribute".into(),
                ));
            }
        }
    };

    Ok(result)
}
