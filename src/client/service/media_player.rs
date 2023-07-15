// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Media player entity specific HA service call logic.

use crate::client::service::{cmd_from_str, get_required_params};
use crate::errors::ServiceError;
use serde_json::{json, Map, Value};
use uc_api::intg::EntityCommand;
use uc_api::MediaPlayerCommand;

pub fn handle_media_player(msg: &EntityCommand) -> Result<(String, Option<Value>), ServiceError> {
    let cmd: MediaPlayerCommand = cmd_from_str(&msg.cmd_id)?;

    let result = match cmd {
        MediaPlayerCommand::On => ("turn_on".into(), None),
        MediaPlayerCommand::Off => ("turn_off".into(), None),
        MediaPlayerCommand::Toggle => ("toggle".into(), None),
        MediaPlayerCommand::PlayPause => ("media_play_pause".into(), None),
        MediaPlayerCommand::Stop => ("media_stop".into(), None),
        MediaPlayerCommand::Previous => ("media_previous_track".into(), None),
        MediaPlayerCommand::Next => ("media_next_track".into(), None),
        MediaPlayerCommand::Seek => {
            let mut data = Map::new();
            let params = get_required_params(msg)?;
            // TODO test and verify seeking! Docs says: platform dependent...
            if let Some(value) = params.get("media_position").and_then(|v| v.as_u64()) {
                data.insert("seek_position".into(), value.into());
            } else {
                return Err(ServiceError::BadRequest(
                    "Invalid or missing params.media_position attribute".into(),
                ));
            }
            ("media_seek".into(), Some(data.into()))
        }
        MediaPlayerCommand::Volume => {
            let mut data = Map::new();
            let params = get_required_params(msg)?;
            if let Some(volume @ 0..=100) = params.get("volume").and_then(|v| v.as_u64()) {
                data.insert("volume_level".into(), (volume as f64 / 100_f64).into());
            } else {
                return Err(ServiceError::BadRequest(
                    "Invalid or missing params.volume attribute".into(),
                ));
            }
            ("volume_set".into(), Some(data.into()))
        }
        MediaPlayerCommand::VolumeUp => ("volume_up".into(), None),
        MediaPlayerCommand::VolumeDown => ("volume_down".into(), None),
        MediaPlayerCommand::FastForward
        | MediaPlayerCommand::Rewind
        | MediaPlayerCommand::MuteToggle => {
            return Err(ServiceError::BadRequest("Not supported".into()))
        }
        MediaPlayerCommand::Mute => (
            "volume_mute".into(),
            Some(json!({ "is_volume_muted": true })),
        ),
        MediaPlayerCommand::Unmute => (
            "volume_mute".into(),
            Some(json!({ "is_volume_muted": false })),
        ),
        MediaPlayerCommand::Repeat => {
            let mut data = Map::new();
            let params = get_required_params(msg)?;
            if let Some(repeat) = params.get("repeat").and_then(|v| v.as_str()) {
                data.insert("repeat".into(), repeat.to_lowercase().into());
            } else {
                return Err(ServiceError::BadRequest(
                    "Invalid or missing params.repeat attribute".into(),
                ));
            }
            ("repeat_set".into(), Some(data.into()))
        }
        MediaPlayerCommand::Shuffle => {
            let mut data = Map::new();
            let params = get_required_params(msg)?;
            if let Some(shuffle) = params.get("shuffle").and_then(|v| v.as_bool()) {
                data.insert("shuffle".into(), shuffle.into());
            } else {
                return Err(ServiceError::BadRequest(
                    "Invalid or missing params.shuffle attribute".into(),
                ));
            }
            ("shuffle_set".into(), Some(data.into()))
        }
        // TODO can we find out related HA entities and forward the command to these? Would we very convenient for the user!
        // E.g. the remote entity which usually comes with a media-player entity as for ATV or LG TV
        MediaPlayerCommand::ChannelUp
        | MediaPlayerCommand::ChannelDown
        | MediaPlayerCommand::CursorUp
        | MediaPlayerCommand::CursorDown
        | MediaPlayerCommand::CursorLeft
        | MediaPlayerCommand::CursorRight
        | MediaPlayerCommand::CursorEnter
        | MediaPlayerCommand::FunctionRed
        | MediaPlayerCommand::FunctionGreen
        | MediaPlayerCommand::FunctionYellow
        | MediaPlayerCommand::FunctionBlue
        | MediaPlayerCommand::Home
        | MediaPlayerCommand::Menu
        | MediaPlayerCommand::Back => return Err(ServiceError::BadRequest("Not supported".into())),
        MediaPlayerCommand::SelectSource => {
            let mut data = Map::new();
            let params = get_required_params(msg)?;
            if let Some(source) = params.get("source").and_then(|v| v.as_str()) {
                data.insert("source".into(), source.into());
            } else {
                return Err(ServiceError::BadRequest(
                    "Invalid or missing params.source attribute".into(),
                ));
            }
            ("select_source".into(), Some(data.into()))
        }
        MediaPlayerCommand::SelectSoundMode => {
            let mut data = Map::new();
            let params = get_required_params(msg)?;
            if let Some(mode) = params.get("sound_mode").and_then(|v| v.as_str()) {
                data.insert("sound_mode".into(), mode.into());
            } else {
                return Err(ServiceError::BadRequest(
                    "Invalid or missing params.sound_mode attribute".into(),
                ));
            }
            ("select_sound_mode".into(), Some(data.into()))
        }
    };

    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::client::service::media_player::handle_media_player;
    use crate::errors::ServiceError;
    use rstest::rstest;
    use serde_json::{json, Map, Value};
    use uc_api::intg::EntityCommand;
    use uc_api::EntityType;

    fn new_entity_command(cmd_id: impl Into<String>, params: Value) -> EntityCommand {
        EntityCommand {
            device_id: None,
            entity_type: EntityType::MediaPlayer,
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
    #[case(json!(0), json!(0.0))] // TODO find a safer way to compare floats, this might blow any time
    #[case(json!(1), json!(0.01))]
    #[case(json!(50), json!(0.5))]
    #[case(json!(100), json!(1.0))]
    fn volume_cmd_returns_proper_request(#[case] volume: Value, #[case] output: Value) {
        let cmd = new_entity_command("volume", json!({ "volume": volume }));
        let result = handle_media_player(&cmd);

        assert!(
            result.is_ok(),
            "Valid value must return Ok, but got: {:?}",
            result.unwrap_err()
        );
        let (cmd, param) = result.unwrap();
        assert_eq!("volume_set", &cmd);
        assert!(param.is_some(), "Param object missing");
        assert_eq!(Some(&output), param.unwrap().get("volume_level"));
    }

    #[rstest]
    #[case(json!(-1))]
    #[case(json!(0.0))]
    #[case(json!(50.0))]
    #[case(json!(101))]
    #[case(json!(200))]
    #[case(json!(true))]
    #[case(json!(false))]
    fn volume_cmd_with_invalid_volume_param_returns_bad_request(#[case] volume: Value) {
        let cmd = new_entity_command("volume", json!({ "volume": volume }));
        let result = handle_media_player(&cmd);

        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "Invalid value must return BadRequest, but got: {:?}",
            result
        );
    }

    #[rstest]
    #[case(Value::Null)]
    #[case(Value::Object(Map::new()))]
    fn volume_cmd_with_invalid_param_object_returns_bad_request(#[case] params: Value) {
        let cmd = new_entity_command("volume", params);
        let result = handle_media_player(&cmd);

        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "Invalid value must return BadRequest, but got: {:?}",
            result
        );
    }
}
