// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Light entity specific HA service call logic.

use crate::client::service::cmd_from_str;
use crate::errors::ServiceError;
use serde_json::{json, Map, Value};
use uc_api::intg::EntityCommand;
use uc_api::LightCommand;

pub(crate) fn handle_light(msg: &EntityCommand) -> Result<(String, Option<Value>), ServiceError> {
    let cmd: LightCommand = cmd_from_str(&msg.cmd_id)?;

    let result = match cmd {
        LightCommand::On => {
            let mut data = Map::new();
            if let Some(params) = msg.params.as_ref() {
                if let Some(brightness @ 0..=255) =
                    params.get("brightness").and_then(|v| v.as_u64())
                {
                    data.insert("brightness".into(), Value::Number(brightness.into()));
                }
                if let Some(color_temp_pct) =
                    params.get("color_temperature").and_then(|v| v.as_u64())
                {
                    // TODO keep an inventory of mired range per light
                    let min_mireds = 150;
                    let max_mireds = 500;
                    let color_temp =
                        color_temp_percent_to_mired(color_temp_pct, min_mireds, max_mireds)?;
                    data.insert("color_temp".into(), Value::Number(color_temp.into()));
                }
                if let Some(hue @ 0..=360) = params.get("hue").and_then(|v| v.as_u64()) {
                    if let Some(saturation @ 0..=255) =
                        params.get("saturation").and_then(|v| v.as_u64())
                    {
                        data.insert("hs_color".into(), json!([hue, saturation * 100 / 255]));
                    }
                }
            }
            ("turn_on".into(), Some(Value::Object(data)))
        }
        LightCommand::Off => ("turn_off".into(), None),
        LightCommand::Toggle => ("Toggle".into(), None),
    };

    Ok(result)
}

fn color_temp_percent_to_mired(
    value: u64,
    min_mireds: u16,
    max_mireds: u16,
) -> Result<u16, ServiceError> {
    if max_mireds <= min_mireds {
        return Err(ServiceError::BadRequest(format!(
            "Invalid min_mireds or max_mireds value! min_mireds={}, max_mireds={}",
            min_mireds, max_mireds
        )));
    }
    if value <= 100 {
        Ok(value as u16 * (max_mireds - min_mireds) / 100 + min_mireds)
    } else {
        Err(ServiceError::BadRequest(format!(
            "Invalid color_temperature value {}: Valid: 0..100",
            value
        )))
    }
}

#[cfg(test)]
mod tests {
    use crate::client::service::light::color_temp_percent_to_mired;
    use crate::errors::ServiceError;
    use rstest::rstest;

    #[test]
    fn color_temp_percent_to_mired_with_invalid_input_returns_err() {
        let result = color_temp_percent_to_mired(101, 150, 500);
        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "Invalid value must return BadRequest, but got: {:?}",
            result
        );
    }

    #[rstest]
    #[case(150, 150)]
    #[case(200, 150)]
    fn color_temp_percent_to_mired_with_invalid_min_max_mireds_returns_err(
        #[case] min_mireds: u16,
        #[case] max_mireds: u16,
    ) {
        let result = color_temp_percent_to_mired(50, min_mireds, max_mireds);
        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "Invalid min_ / max_mireds value must return BadRequest"
        );
    }

    #[rstest]
    #[case(0, 150)]
    #[case(1, 153)]
    #[case(50, 325)]
    #[case(99, 496)]
    #[case(100, 500)]
    fn color_temp_percent_to_mired_returns_scaled_values(
        #[case] input: u64,
        #[case] expected: u16,
    ) {
        let min_mireds = 150;
        let max_mireds = 500;
        let result = color_temp_percent_to_mired(input, min_mireds, max_mireds);

        assert_eq!(Ok(expected), result);
    }
}
