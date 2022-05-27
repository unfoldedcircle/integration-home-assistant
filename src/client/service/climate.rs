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
                    data.insert("hvac_mode".into(), mode.to_lowercase().into());
                }
                "FAN" => {
                    data.insert("hvac_mode".into(), "fan_only".into());
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

#[cfg(test)]
mod tests {
    use crate::client::service::climate::handle_climate;
    use rstest::rstest;
    use serde_json::{json, Value};
    use uc_api::intg::EntityCommand;

    #[test]
    fn turn_on() {
        let msg_data = json!({
            "cmd_id": "on",
            "entity_id": "climate.bathroom_floor_heating_mode",
            "entity_type": "climate"
        });
        let (cmd, data) = map_msg_data(msg_data);
        assert_eq!("turn_on", cmd);
        assert!(data.is_none(), "no cmd data allowed");
    }

    #[test]
    fn turn_off() {
        let msg_data = json!({
            "cmd_id": "off",
            "entity_id": "climate.bathroom_floor_heating_mode",
            "entity_type": "climate"
        });
        let (cmd, data) = map_msg_data(msg_data);
        assert_eq!("turn_off", cmd);
        assert!(data.is_none(), "no cmd data allowed");
    }

    #[rstest]
    #[case("OFF", "off")]
    #[case("HEAT", "heat")]
    #[case("COOL", "cool")]
    #[case("HEAT_COOL", "heat_cool")]
    #[case("AUTO", "auto")]
    #[case("FAN", "fan_only")]
    fn hvac_mode(#[case] uc_cmd: &str, #[case] ha_cmd: &str) {
        let msg_data = json!({
            "cmd_id": "hvac_mode",
            "entity_id": "climate.bathroom_floor_heating_mode",
            "entity_type": "climate",
            "params": {
                "hvac_mode": uc_cmd
            }
        });
        let (cmd, data) = map_msg_data(msg_data);
        assert_eq!("set_hvac_mode", cmd);
        assert!(data.is_some(), "cmd data expected");
        let data = data.unwrap();
        assert_eq!(Some(&json!(ha_cmd)), data.get("hvac_mode"));
    }

    #[test]
    fn set_temperature() {
        let msg_data = json!({
            "cmd_id": "target_temperature",
            "entity_id": "climate.bathroom_floor_heating_mode",
            "entity_type": "climate",
            "params": {
              "temperature": 22.5
            }
        });
        let (cmd, data) = map_msg_data(msg_data);
        assert_eq!("set_temperature", cmd);
        assert!(data.is_some(), "cmd data expected");
        let data = data.unwrap();
        assert_eq!(Some(&json!(22.5)), data.get("temperature"));
    }

    fn map_msg_data(msg_data: Value) -> (String, Option<Value>) {
        let cmd: EntityCommand = serde_json::from_value(msg_data).expect("invalid test data");
        let result = handle_climate(&cmd);
        assert!(
            result.is_ok(),
            "Expected successful cmd mapping but got: {:?}",
            result.unwrap_err()
        );
        result.unwrap()
    }
}
