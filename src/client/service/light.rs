// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Light entity specific HA service call logic.

use std::str::FromStr;

use serde_json::{Map, Number, Value};

use uc_api::LightCommand;

use crate::client::messages::CallService;
use crate::errors::ServiceError;

pub(crate) fn handle_light(msg: &CallService) -> Result<(String, Option<Value>), ServiceError> {
    let cmd = LightCommand::from_str(&msg.command.cmd_id)?;

    let result = match cmd {
        LightCommand::On => {
            let mut data = Map::new();
            if let Some(params) = msg.command.params.as_ref().and_then(|v| v.as_object()) {
                if let Some(brightness) = params.get("brightness").and_then(|v| v.as_u64()) {
                    // FIXME brightness_pct might no longer be supported with newer HA versions!
                    data.insert(
                        "brightness_pct".to_string(),
                        Value::Number(Number::from(brightness * 100 / 255)),
                    );
                }
                if let Some(_color_temp) = params.get("color_temperature").and_then(|v| v.as_u64())
                {
                    todo!()
                }
                if let Some(_hue) = params.get("hue").and_then(|v| v.as_u64()) {
                    todo!()
                }
                if let Some(_saturation) = params.get("saturation").and_then(|v| v.as_u64()) {
                    todo!()
                }
            }
            ("turn_on".to_string(), Some(Value::Object(data)))
        }
        LightCommand::Off => ("turn_off".to_string(), None),
        LightCommand::Toggle => ("Toggle".to_string(), None),
    };

    Ok(result)
}
