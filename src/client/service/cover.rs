// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Cover entity specific HA service call logic.

use crate::client::cmd_from_str;
use crate::errors::ServiceError;
use serde_json::{Map, Value};
use uc_api::CoverCommand;
use uc_api::intg::EntityCommand;

pub(crate) fn handle_cover(msg: &EntityCommand) -> Result<(String, Option<Value>), ServiceError> {
    let cmd: CoverCommand = cmd_from_str(&msg.cmd_id)?;

    let result = match cmd {
        CoverCommand::Open => ("open_cover".into(), None),
        CoverCommand::Close => ("close_cover".into(), None),
        CoverCommand::Stop => ("stop_cover".into(), None),
        CoverCommand::Position => {
            let mut data = Map::new();
            if let Some(params) = msg.params.as_ref()
                && let Some(pos @ 0..=100) = params.get("position").and_then(|v| v.as_u64())
            {
                data.insert("position".into(), Value::Number(pos.into()));
            }
            ("set_cover_position".into(), Some(data.into()))
        } // TODO implement tilt command #6
    };

    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::client::service::cover::handle_cover;
    use rstest::rstest;
    use serde_json::{Map, Value, json};
    use uc_api::EntityType;
    use uc_api::intg::EntityCommand;

    fn new_entity_command(cmd_id: impl Into<String>, params: Value) -> EntityCommand {
        EntityCommand {
            device_id: None,
            entity_type: EntityType::Cover,
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
    #[case("open", "open_cover")]
    #[case("close", "close_cover")]
    #[case("stop", "stop_cover")]
    fn simple_commands_return_proper_service_call(
        #[case] cmd_id: &str,
        #[case] expected_service: &str,
    ) {
        let cmd = new_entity_command(cmd_id, Value::Null);
        let result = handle_cover(&cmd);

        assert!(
            result.is_ok(),
            "Valid command must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!(expected_service, &service);
        assert!(
            param.is_none(),
            "Simple commands should not have parameters"
        );
    }

    #[rstest]
    #[case(json!(0), json!(0))]
    #[case(json!(1), json!(1))]
    #[case(json!(50), json!(50))]
    #[case(json!(100), json!(100))]
    fn position_cmd_with_valid_position_returns_proper_request(
        #[case] position: Value,
        #[case] expected: Value,
    ) {
        let cmd = new_entity_command("position", json!({ "position": position }));
        let result = handle_cover(&cmd);

        assert!(
            result.is_ok(),
            "Valid position must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("set_cover_position", &service);
        assert!(param.is_some(), "Position command should have parameters");
        assert_eq!(Some(&expected), param.unwrap().get("position"));
    }

    #[test]
    fn position_cmd_without_params_returns_empty_data() {
        let cmd = new_entity_command("position", Value::Null);
        let result = handle_cover(&cmd);

        assert!(
            result.is_ok(),
            "Position command without params should return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("set_cover_position", &service);
        assert!(
            param.is_some(),
            "Position command should have parameters object"
        );
        let param_obj = param.unwrap();
        assert!(
            param_obj.as_object().unwrap().is_empty(),
            "Parameters should be empty when no valid position provided"
        );
    }

    #[rstest]
    #[case(json!(-1))]
    #[case(json!(101))]
    #[case(json!(200))]
    #[case(json!(0.0))]
    #[case(json!(50.5))]
    #[case(json!(true))]
    #[case(json!(false))]
    #[case(json!("50"))]
    fn position_cmd_with_invalid_position_returns_empty_data(#[case] position: Value) {
        let cmd = new_entity_command("position", json!({ "position": position }));
        let result = handle_cover(&cmd);

        assert!(
            result.is_ok(),
            "Position command with invalid position should return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("set_cover_position", &service);
        assert!(
            param.is_some(),
            "Position command should have parameters object"
        );
        let param_obj = param.unwrap();
        assert!(
            param_obj.as_object().unwrap().is_empty(),
            "Parameters should be empty when invalid position provided"
        );
    }

    #[rstest]
    #[case(Value::Object(Map::new()))]
    #[case(json!({ "other_param": 50 }))]
    fn position_cmd_with_missing_position_param_returns_empty_data(#[case] params: Value) {
        let cmd = new_entity_command("position", params);
        let result = handle_cover(&cmd);

        assert!(
            result.is_ok(),
            "Position command with missing position param should return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("set_cover_position", &service);
        assert!(
            param.is_some(),
            "Position command should have parameters object"
        );
        let param_obj = param.unwrap();
        assert!(
            param_obj.as_object().unwrap().is_empty(),
            "Parameters should be empty when position param is missing"
        );
    }

    #[test]
    fn invalid_command_returns_error() {
        let cmd = new_entity_command("invalid_command", Value::Null);
        let result = handle_cover(&cmd);

        assert!(result.is_err(), "Invalid command should return error");
        // The specific error type depends on the cmd_from_str implementation
        // but it should be some kind of ServiceError
    }
}
