// Copyright (c) 2024 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Remote entity specific HA service call logic.

use crate::client::{cmd_from_str, get_required_params};
use crate::errors::ServiceError;
use serde_json::{Map, Value};
use uc_api::intg::{EntityCommand, IntgRemoteCommand};

pub(crate) fn handle_remote(msg: &EntityCommand) -> Result<(String, Option<Value>), ServiceError> {
    let cmd: IntgRemoteCommand = cmd_from_str(&msg.cmd_id)?;

    let result = match cmd {
        IntgRemoteCommand::On => ("turn_on".into(), None),
        IntgRemoteCommand::Off => ("turn_off".into(), None),
        IntgRemoteCommand::Toggle => ("toggle".into(), None),
        IntgRemoteCommand::SendCmd => create_command(msg, "command")?,
        IntgRemoteCommand::SendCmdSequence => create_command(msg, "sequence")?,
        IntgRemoteCommand::StopSend => {
            return Err(ServiceError::BadRequest(
                "stop_send command is not supported".into(),
            ));
        }
    };

    Ok(result)
}

fn create_command(msg: &EntityCommand, cmd: &str) -> Result<(String, Option<Value>), ServiceError> {
    if cmd.trim().is_empty() {
        return Err(ServiceError::BadRequest("empty command".into()));
    }
    let mut data = Map::new();
    let params = get_required_params(msg)?;
    if let Some(value) = params.get(cmd)
        && (cmd == "sequence" && value.is_array() || cmd == "command" && value.is_string())
    {
        if cmd == "command"
            && value
                .as_str()
                .map(|v| v.trim().is_empty())
                .unwrap_or_default()
        {
            return Err(ServiceError::BadRequest("empty command".into()));
        }
        data.insert("command".into(), value.clone());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::ServiceError;
    use rstest::rstest;
    use serde_json::{Value, json};
    use uc_api::EntityType;
    use uc_api::intg::EntityCommand;

    fn new_entity_command(cmd_id: impl Into<String>, params: Value) -> EntityCommand {
        EntityCommand {
            device_id: None,
            entity_type: EntityType::Remote,
            entity_id: "test".into(),
            cmd_id: cmd_id.into(),
            params: if params.is_object() {
                Some(params.as_object().unwrap().clone())
            } else {
                None
            },
        }
    }

    #[rstest]
    #[case("on", "turn_on")]
    #[case("off", "turn_off")]
    #[case("toggle", "toggle")]
    fn basic_commands_return_correct_ha_service(
        #[case] cmd_id: &str,
        #[case] expected_service: &str,
    ) {
        let cmd = new_entity_command(cmd_id, json!({}));
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "Valid command must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!(expected_service, &service);
        assert_eq!(None, param);
    }

    #[test]
    fn stop_send_command_is_not_supported_and_returns_bad_request() {
        let cmd = new_entity_command("stop_send", json!({}));
        let result = handle_remote(&cmd);

        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "stop_send command must return BadRequest, but got: {:?}",
            result
        );

        if let Err(ServiceError::BadRequest(msg)) = result {
            assert!(msg.contains("stop_send command is not supported"));
        }
    }

    #[test]
    fn send_cmd_with_valid_command_returns_send_command() {
        let cmd = new_entity_command(
            "send_cmd",
            json!({
                "command": "power_on"
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "Valid send_cmd must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("send_command", &service);
        assert!(param.is_some());

        let data = param.unwrap();
        assert_eq!(Some(&json!("power_on")), data.get("command"));
    }

    #[test]
    fn send_cmd_sequence_with_valid_array_returns_send_command() {
        let cmd = new_entity_command(
            "send_cmd_sequence",
            json!({
                "sequence": ["power_on", "input_hdmi1", "volume_up"]
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "Valid send_cmd_sequence must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("send_command", &service);
        assert!(param.is_some());

        let data = param.unwrap();
        assert_eq!(
            Some(&json!(["power_on", "input_hdmi1", "volume_up"])),
            data.get("command")
        );
    }

    #[test]
    fn send_cmd_with_repeat_parameter_adds_num_repeats() {
        let cmd = new_entity_command(
            "send_cmd",
            json!({
                "command": "volume_up",
                "repeat": 3
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "Valid send_cmd with repeat must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("send_command", &service);
        assert!(param.is_some());

        let data = param.unwrap();
        assert_eq!(Some(&json!("volume_up")), data.get("command"));
        assert_eq!(Some(&json!(3)), data.get("num_repeats"));
    }

    #[test]
    fn send_cmd_with_delay_parameter_converts_to_seconds() {
        let cmd = new_entity_command(
            "send_cmd",
            json!({
                "command": "power_on",
                "delay": 1500  // milliseconds
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "Valid send_cmd with delay must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("send_command", &service);
        assert!(param.is_some());

        let data = param.unwrap();
        assert_eq!(Some(&json!("power_on")), data.get("command"));
        assert_eq!(Some(&json!(1.5)), data.get("delay_secs"));
    }

    #[test]
    fn send_cmd_with_hold_parameter_converts_to_seconds() {
        let cmd = new_entity_command(
            "send_cmd",
            json!({
                "command": "power_button",
                "hold": 2000  // milliseconds
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "Valid send_cmd with hold must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("send_command", &service);
        assert!(param.is_some());

        let data = param.unwrap();
        assert_eq!(Some(&json!("power_button")), data.get("command"));
        assert_eq!(Some(&json!(2.0)), data.get("hold_secs"));
    }

    #[test]
    fn send_cmd_with_all_parameters() {
        let cmd = new_entity_command(
            "send_cmd",
            json!({
                "command": "complex_command",
                "repeat": 2,
                "delay": 500,
                "hold": 1000
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "Valid send_cmd with all parameters must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("send_command", &service);
        assert!(param.is_some());

        let data = param.unwrap();
        assert_eq!(Some(&json!("complex_command")), data.get("command"));
        assert_eq!(Some(&json!(2)), data.get("num_repeats"));
        assert_eq!(Some(&json!(0.5)), data.get("delay_secs"));
        assert_eq!(Some(&json!(1.0)), data.get("hold_secs"));
    }

    #[test]
    fn send_cmd_sequence_with_all_parameters() {
        let cmd = new_entity_command(
            "send_cmd_sequence",
            json!({
                "sequence": ["cmd1", "cmd2"],
                "repeat": 3,
                "delay": 250,
                "hold": 500
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "Valid send_cmd_sequence with all parameters must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("send_command", &service);
        assert!(param.is_some());

        let data = param.unwrap();
        assert_eq!(Some(&json!(["cmd1", "cmd2"])), data.get("command"));
        assert_eq!(Some(&json!(3)), data.get("num_repeats"));
        assert_eq!(Some(&json!(0.25)), data.get("delay_secs"));
        assert_eq!(Some(&json!(0.5)), data.get("hold_secs"));
    }

    #[test]
    fn send_cmd_without_command_parameter_returns_bad_request() {
        let cmd = new_entity_command("send_cmd", json!({}));
        let result = handle_remote(&cmd);

        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "send_cmd without command must return BadRequest, but got: {:?}",
            result
        );

        if let Err(ServiceError::BadRequest(msg)) = result {
            assert!(msg.contains("Invalid or missing attribute: params.command"));
        }
    }

    #[test]
    fn send_cmd_sequence_without_sequence_parameter_returns_bad_request() {
        let cmd = new_entity_command("send_cmd_sequence", json!({}));
        let result = handle_remote(&cmd);

        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "send_cmd_sequence without sequence must return BadRequest, but got: {:?}",
            result
        );

        if let Err(ServiceError::BadRequest(msg)) = result {
            assert!(msg.contains("Invalid or missing attribute: params.sequence"));
        }
    }

    #[rstest]
    #[case(json!(123))]
    #[case(json!(true))]
    #[case(json!(["array", "not", "string"]))]
    #[case(json!(null))]
    fn send_cmd_with_invalid_command_type_returns_bad_request(#[case] command: Value) {
        let cmd = new_entity_command(
            "send_cmd",
            json!({
                "command": command
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "send_cmd with invalid command type must return BadRequest, but got: {:?}",
            result
        );
    }

    #[rstest]
    #[case(json!("string"))]
    #[case(json!(123))]
    #[case(json!(true))]
    #[case(json!(null))]
    fn send_cmd_sequence_with_invalid_sequence_type_returns_bad_request(#[case] sequence: Value) {
        let cmd = new_entity_command(
            "send_cmd_sequence",
            json!({
                "sequence": sequence
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "send_cmd_sequence with invalid sequence type must return BadRequest, but got: {:?}",
            result
        );
    }

    #[rstest]
    #[case(json!(""))]
    #[case(json!(" "))]
    #[case(json!("\n"))]
    #[case(json!("\t"))]
    fn send_cmd_with_empty_string_command_returns_bad_request(#[case] command: Value) {
        let cmd = new_entity_command(
            "send_cmd",
            json!({
                "command": command
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "send_cmd_sequence with invalid sequence type must return BadRequest, but got: {:?}",
            result
        );
    }

    // TODO should we really allow an empty sequence?
    #[test]
    fn send_cmd_sequence_with_empty_array_returns_ok() {
        let cmd = new_entity_command(
            "send_cmd_sequence",
            json!({
                "sequence": []
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "send_cmd_sequence with empty array should return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("send_command", &service);
        assert!(param.is_some());

        let data = param.unwrap();
        assert_eq!(Some(&json!([])), data.get("command"));
    }

    #[rstest]
    #[case(json!("not_a_number"))]
    #[case(json!(true))]
    #[case(json!([])) ]
    #[case(json!(-1))]
    fn send_cmd_with_invalid_repeat_type_ignores_parameter(#[case] repeat: Value) {
        let cmd = new_entity_command(
            "send_cmd",
            json!({
                "command": "test_cmd",
                "repeat": repeat
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "send_cmd with invalid repeat type should still work, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("send_command", &service);
        assert!(param.is_some());

        let data = param.unwrap();
        assert_eq!(Some(&json!("test_cmd")), data.get("command"));
        // Invalid repeat parameter should be ignored
        assert!(data.get("num_repeats").is_none());
    }

    #[rstest]
    #[case(json!("not_a_number"))]
    #[case(json!(true))]
    #[case(json!([]))]
    #[case(json!(-1))]
    fn send_cmd_with_invalid_delay_type_ignores_parameter(#[case] delay: Value) {
        let cmd = new_entity_command(
            "send_cmd",
            json!({
                "command": "test_cmd",
                "delay": delay
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "send_cmd with invalid delay type should still work, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("send_command", &service);
        assert!(param.is_some());

        let data = param.unwrap();
        assert_eq!(Some(&json!("test_cmd")), data.get("command"));
        // Invalid delay parameter should be ignored
        assert!(data.get("delay_secs").is_none());
    }

    #[rstest]
    #[case(json!("not_a_number"))]
    #[case(json!(true))]
    #[case(json!([]))]
    #[case(json!(-1))]
    fn send_cmd_with_invalid_hold_type_ignores_parameter(#[case] hold: Value) {
        let cmd = new_entity_command(
            "send_cmd",
            json!({
                "command": "test_cmd",
                "hold": hold
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "send_cmd with invalid hold type should still work, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("send_command", &service);
        assert!(param.is_some());

        let data = param.unwrap();
        assert_eq!(Some(&json!("test_cmd")), data.get("command"));
        // Invalid hold parameter should be ignored
        assert!(data.get("hold_secs").is_none());
    }

    #[test]
    fn send_cmd_with_zero_delay_converts_correctly() {
        let cmd = new_entity_command(
            "send_cmd",
            json!({
                "command": "test_cmd",
                "delay": 0
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "send_cmd with zero delay should work, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("send_command", &service);
        assert!(param.is_some());

        let data = param.unwrap();
        assert_eq!(Some(&json!("test_cmd")), data.get("command"));
        assert_eq!(Some(&json!(0.0)), data.get("delay_secs"));
    }

    #[test]
    fn send_cmd_with_zero_hold_converts_correctly() {
        let cmd = new_entity_command(
            "send_cmd",
            json!({
                "command": "test_cmd",
                "hold": 0
            }),
        );
        let result = handle_remote(&cmd);

        assert!(
            result.is_ok(),
            "send_cmd with zero hold should work, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("send_command", &service);
        assert!(param.is_some());

        let data = param.unwrap();
        assert_eq!(Some(&json!("test_cmd")), data.get("command"));
        assert_eq!(Some(&json!(0.0)), data.get("hold_secs"));
    }

    #[test]
    fn send_cmd_without_params_returns_bad_request() {
        let cmd = EntityCommand {
            device_id: None,
            entity_type: EntityType::Remote,
            entity_id: "test".into(),
            cmd_id: "send_cmd".into(),
            params: None,
        };
        let result = handle_remote(&cmd);

        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "send_cmd without params must return BadRequest, but got: {:?}",
            result
        );
    }

    #[rstest]
    #[case("invalid_command")]
    #[case("unknown")]
    #[case("")]
    fn invalid_command_ids_return_bad_request(#[case] cmd_id: &str) {
        let cmd = new_entity_command(cmd_id, json!({}));
        let result = handle_remote(&cmd);

        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "Invalid command '{}' must return BadRequest, but got: {:?}",
            cmd_id,
            result
        );
    }
}
