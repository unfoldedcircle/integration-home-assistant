// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Button entity specific HA service call logic.

use crate::client::cmd_from_str;
use crate::errors::ServiceError;
use serde_json::Value;
use uc_api::ButtonCommand;
use uc_api::intg::EntityCommand;

pub(crate) fn handle_button(msg: &EntityCommand) -> Result<(String, Option<Value>), ServiceError> {
    let cmd: ButtonCommand = cmd_from_str(&msg.cmd_id)?;

    let entity: Vec<&str> = msg.entity_id.split('.').collect();

    let service_call: &str = match entity[0] {
        "script" => entity[1],
        "scene" => "turn_on",
        &_ => "press",
    };

    let result = match cmd {
        ButtonCommand::Push => (service_call.into(), None),
    };

    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::client::service::button::handle_button;
    use rstest::rstest;
    use uc_api::EntityType;
    use uc_api::intg::EntityCommand;

    fn new_entity_command(
        entity_id: impl Into<String>,
        cmd_id: impl Into<String>,
    ) -> EntityCommand {
        EntityCommand {
            device_id: None,
            entity_type: EntityType::Button,
            entity_id: entity_id.into(),
            cmd_id: cmd_id.into(),
            params: None,
        }
    }

    #[rstest]
    #[case("script.foobar", "foobar")]
    #[case("script.turn_on_lights", "turn_on_lights")]
    #[case("script.complex_script_123", "complex_script_123")]
    fn script_entities_use_script_name_as_service(
        #[case] entity_id: &str,
        #[case] expected_service: &str,
    ) {
        let cmd = new_entity_command(entity_id, "push");
        let result = handle_button(&cmd);

        assert!(
            result.is_ok(),
            "Valid script entity must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!(expected_service, &service);
        assert!(
            param.is_none(),
            "Button commands should not have parameters"
        );
    }

    #[rstest]
    #[case("scene.morning", "turn_on")]
    #[case("scene.evening", "turn_on")]
    #[case("scene.party_mode", "turn_on")]
    fn scene_entities_use_turn_on_service(#[case] entity_id: &str, #[case] expected_service: &str) {
        let cmd = new_entity_command(entity_id, "push");
        let result = handle_button(&cmd);

        assert!(
            result.is_ok(),
            "Valid scene entity must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!(expected_service, &service);
        assert!(
            param.is_none(),
            "Button commands should not have parameters"
        );
    }

    #[rstest]
    #[case("button.doorbell", "press")]
    #[case("input_button.test_button", "press")]
    #[case("automation.my_automation", "press")]
    #[case("switch.some_switch", "press")]
    #[case("light.some_light", "press")]
    #[case("unknown.entity_type", "press")]
    fn other_entities_use_press_service(#[case] entity_id: &str, #[case] expected_service: &str) {
        let cmd = new_entity_command(entity_id, "push");
        let result = handle_button(&cmd);

        assert!(
            result.is_ok(),
            "Valid entity must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!(expected_service, &service);
        assert!(
            param.is_none(),
            "Button commands should not have parameters"
        );
    }

    #[test]
    fn entity_id_without_domain_separator_uses_press_service() {
        let cmd = new_entity_command("no_separator_entity", "push");
        let result = handle_button(&cmd);

        assert!(
            result.is_ok(),
            "Entity without separator must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("press", &service);
        assert!(
            param.is_none(),
            "Button commands should not have parameters"
        );
    }

    #[test]
    fn script_entity_with_multiple_dots_uses_first_part_after_domain() {
        let cmd = new_entity_command("script.my.complex.script.name", "push");
        let result = handle_button(&cmd);

        assert!(
            result.is_ok(),
            "Script with multiple dots must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("my", &service);
        assert!(
            param.is_none(),
            "Button commands should not have parameters"
        );
    }

    #[test]
    fn scene_entity_with_multiple_dots_uses_turn_on() {
        let cmd = new_entity_command("scene.my.complex.scene.name", "push");
        let result = handle_button(&cmd);

        assert!(
            result.is_ok(),
            "Scene with multiple dots must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (service, param) = result.unwrap();
        assert_eq!("turn_on", &service);
        assert!(
            param.is_none(),
            "Button commands should not have parameters"
        );
    }

    #[test]
    fn invalid_command_returns_error() {
        let cmd = new_entity_command("button.test", "invalid_command");
        let result = handle_button(&cmd);

        assert!(result.is_err(), "Invalid command should return error");
    }

    #[test]
    fn push_command_is_case_sensitive() {
        let cmd = new_entity_command("button.test", "push");
        let result = handle_button(&cmd);

        assert!(
            result.is_ok(),
            "Lowercase 'push' command must work, but got: {:?}",
            result.unwrap_err()
        );

        let cmd_upper = new_entity_command("button.test", "PUSH");
        let result_upper = handle_button(&cmd_upper);

        assert!(
            result_upper.is_err(),
            "Uppercase 'PUSH' command should return error due to case sensitivity"
        );
    }
}
