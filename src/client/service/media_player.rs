// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Media player entity specific HA service call logic.

use serde_json::{json, Map, Value};
use uc_api::MediaPlayerCommand;

use crate::client::messages::CallService;
use crate::client::service::cmd_from_str;
use crate::errors::ServiceError;

pub fn handle_media_player(msg: &CallService) -> Result<(String, Option<Value>), ServiceError> {
    let cmd: MediaPlayerCommand = cmd_from_str(&msg.command.cmd_id)?;

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
            if let Some(params) = msg.command.params.as_ref() {
                // TODO test and verify seeking! Docs says: platform dependent...
                if let Some(value) = params.get("media_position").and_then(|v| v.as_u64()) {
                    data.insert("seek_position".into(), Value::Number(value.into()));
                }
            }
            ("media_seek".into(), Some(data.into()))
        }
        MediaPlayerCommand::Volume => {
            let mut data = Map::new();
            if let Some(params) = msg.command.params.as_ref() {
                if let Some(volume @ 0..=100) = params.get("volume").and_then(|v| v.as_u64()) {
                    data.insert("volume_level".into(), Value::Number(volume.into()));
                }
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
            if let Some(params) = msg.command.params.as_ref() {
                if let Some(repeat) = params.get("repeat").and_then(|v| v.as_str()) {
                    data.insert("repeat".into(), repeat.to_lowercase().into());
                }
            }
            ("repeat_set".into(), Some(data.into()))
        }
        MediaPlayerCommand::Shuffle => {
            let mut data = Map::new();
            if let Some(params) = msg.command.params.as_ref() {
                if let Some(shuffle) = params.get("shuffle").and_then(|v| v.as_bool()) {
                    data.insert("shuffle".into(), shuffle.into());
                }
            }
            ("shuffle_set".into(), Some(data.into()))
        }
    };

    Ok(result)
}
