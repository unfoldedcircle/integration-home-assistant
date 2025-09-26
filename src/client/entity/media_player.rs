// Copyright (c) 2022 {person OR org} <{email}>
// SPDX-License-Identifier: MPL-2.0

//! Media player entity specific logic.

use crate::client::event::convert_ha_onoff_state;
use crate::client::model::EventData;
use crate::errors::ServiceError;
use crate::util::json;
use log::error;
use serde_json::{Map, Value};
use std::collections::HashMap;
use uc_api::intg::{AvailableIntgEntity, EntityChange};
use uc_api::{EntityType, MediaPlayerDeviceClass, MediaPlayerFeature};
use url::Url;

// https://developers.home-assistant.io/docs/core/entity/media-player#supported-features
pub const SUPPORT_PAUSE: u32 = 1;
pub const SUPPORT_SEEK: u32 = 2;
pub const SUPPORT_VOLUME_SET: u32 = 4;
pub const SUPPORT_VOLUME_MUTE: u32 = 8;
pub const SUPPORT_PREVIOUS_TRACK: u32 = 16;
pub const SUPPORT_NEXT_TRACK: u32 = 32;
pub const SUPPORT_TURN_ON: u32 = 128;
pub const SUPPORT_TURN_OFF: u32 = 256;
// pub const SUPPORT_PLAY_MEDIA: u32 = 512;
pub const SUPPORT_VOLUME_STEP: u32 = 1024;
pub const SUPPORT_SELECT_SOURCE: u32 = 2048;
pub const SUPPORT_STOP: u32 = 4096;
// pub const SUPPORT_CLEAR_PLAYLIST: u32 = 8192;
pub const SUPPORT_PLAY: u32 = 16384;
pub const SUPPORT_SHUFFLE_SET: u32 = 32768;
pub const SUPPORT_SELECT_SOUND_MODE: u32 = 65536;
// pub const SUPPORT_BROWSE_MEDIA: u32 = 131072;
pub const SUPPORT_REPEAT_SET: u32 = 262144;
// pub const SUPPORT_GROUPING: u32 = 524288;

pub(crate) fn map_media_player_attributes(
    server: &Url,
    _entity_id: &str,
    state: &str,
    ha_attr: Option<&mut Map<String, Value>>,
) -> Result<Map<String, Value>, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(8);

    let state = match state {
        "playing" | "paused" | "standby" | "buffering" => state.to_uppercase().into(),
        "idle" => "ON".into(),
        _ => convert_ha_onoff_state(state)?,
    };
    attributes.insert("state".into(), state);

    if let Some(ha_attr) = ha_attr {
        if let Some(value) = ha_attr.get("volume_level").and_then(|v| v.as_f64()) {
            attributes.insert("volume".into(), ((value * 100.0).round() as u64).into());
        }
        json::move_value(ha_attr, &mut attributes, "is_volume_muted", "muted");
        json::move_entry(ha_attr, &mut attributes, "media_position");
        json::move_entry(ha_attr, &mut attributes, "media_position_updated_at");
        json::move_entry(ha_attr, &mut attributes, "media_duration");
        json::move_entry(ha_attr, &mut attributes, "media_title");
        json::move_entry(ha_attr, &mut attributes, "media_artist");
        json::move_value(ha_attr, &mut attributes, "media_album_name", "media_album");
        if let Some(value) = ha_attr.get("media_content_type").and_then(|v| v.as_str()) {
            attributes.insert("media_type".into(), value.to_uppercase().into());
        }
        json::move_entry(ha_attr, &mut attributes, "shuffle");
        if let Some(value) = ha_attr.get("repeat").and_then(|v| v.as_str()) {
            attributes.insert("repeat".into(), value.to_uppercase().into());
        }
        json::move_entry(ha_attr, &mut attributes, "source");
        json::move_entry(ha_attr, &mut attributes, "source_list");
        json::move_entry(ha_attr, &mut attributes, "sound_mode");
        json::move_entry(ha_attr, &mut attributes, "sound_mode_list");

        if let Some(value) = ha_attr.get("entity_picture").and_then(|v| v.as_str()) {
            // let's hope it's only http, https or a local path :-)
            if value.starts_with("http") {
                attributes.insert("media_image_url".into(), value.into());
            } else if value.starts_with('/') {
                // `url.set_path(value)` doesn't work since the HA path contains query params as well
                // or we'd have to decode `%3F` -> `?` (and maybe other chars as well).
                // Let's try the simple (and dangerous) approach first which also worked in YIO v1
                attributes.insert(
                    "media_image_url".into(),
                    format!(
                        "{}://{}:{}{}",
                        server.scheme(),
                        server.host_str().unwrap_or_default(),
                        server.port_or_known_default().unwrap_or_default(),
                        value
                    )
                    .into(),
                );
            } else {
                error!("Unexpected entity_picture format: {value}");
            }
        }
    }

    Ok(attributes)
}

pub(crate) fn media_player_event_to_entity_change(
    server: &Url,
    mut data: EventData,
) -> Result<EntityChange, ServiceError> {
    let attributes = map_media_player_attributes(
        server,
        &data.entity_id,
        &data.new_state.state,
        data.new_state.attributes.as_mut(),
    )?;

    Ok(EntityChange {
        device_id: None,
        entity_type: EntityType::MediaPlayer,
        entity_id: data.entity_id,
        attributes,
    })
}

pub(crate) fn convert_media_player_entity(
    server: &Url,
    entity_id: String,
    state: String,
    ha_attr: &mut Map<String, Value>,
) -> Result<AvailableIntgEntity, ServiceError> {
    let friendly_name = ha_attr.get("friendly_name").and_then(|v| v.as_str());
    let name = HashMap::from([("en".into(), friendly_name.unwrap_or(&entity_id).into())]);
    let device_class = ha_attr
        .get("device_class")
        .and_then(|v| v.as_str())
        .and_then(|v| MediaPlayerDeviceClass::try_from(v).ok())
        .map(|v| v.to_string());

    // handle features
    let supported_features = ha_attr
        .get("supported_features")
        .and_then(|v| v.as_u64())
        .unwrap_or_default() as u32;
    let mut media_feats = Vec::with_capacity(16);

    if supported_features & (SUPPORT_TURN_ON | SUPPORT_TURN_OFF) > 0 {
        media_feats.push(MediaPlayerFeature::OnOff);
    }
    if supported_features & SUPPORT_VOLUME_SET > 0 {
        media_feats.push(MediaPlayerFeature::Volume);
    }
    if supported_features & SUPPORT_VOLUME_STEP > 0 {
        media_feats.push(MediaPlayerFeature::VolumeUpDown);
    }
    if supported_features & SUPPORT_SELECT_SOURCE > 0 {
        media_feats.push(MediaPlayerFeature::SelectSource);
    }
    if supported_features & SUPPORT_VOLUME_MUTE > 0 {
        // HASS media player doesn't support mute toggle!
        media_feats.push(MediaPlayerFeature::Mute);
        media_feats.push(MediaPlayerFeature::Unmute);
    }
    if supported_features & (SUPPORT_PAUSE | SUPPORT_PLAY) > 0 {
        media_feats.push(MediaPlayerFeature::PlayPause);
    }
    if supported_features & SUPPORT_STOP > 0 {
        media_feats.push(MediaPlayerFeature::Stop);
    }
    if supported_features & SUPPORT_NEXT_TRACK > 0 {
        media_feats.push(MediaPlayerFeature::Next);
    }
    if supported_features & SUPPORT_PREVIOUS_TRACK > 0 {
        media_feats.push(MediaPlayerFeature::Previous);
    }
    if supported_features & SUPPORT_REPEAT_SET > 0 {
        media_feats.push(MediaPlayerFeature::Repeat);
    }
    if supported_features & SUPPORT_SHUFFLE_SET > 0 {
        media_feats.push(MediaPlayerFeature::Shuffle);
    }
    if supported_features & SUPPORT_SELECT_SOUND_MODE > 0 {
        media_feats.push(MediaPlayerFeature::SelectSoundMode);
    }
    if supported_features & SUPPORT_SEEK > 0 {
        media_feats.push(MediaPlayerFeature::Seek);
        media_feats.push(MediaPlayerFeature::MediaDuration);
        media_feats.push(MediaPlayerFeature::MediaPosition);
    }
    media_feats.push(MediaPlayerFeature::MediaTitle);
    media_feats.push(MediaPlayerFeature::MediaArtist);
    media_feats.push(MediaPlayerFeature::MediaAlbum);
    media_feats.push(MediaPlayerFeature::MediaImageUrl);
    media_feats.push(MediaPlayerFeature::MediaType);

    /* TODO from YIO v1
    features.push("APP_NAME"); ???
     */

    // Note: volume_steps doesn't seem to be retrievable from HA (#14)

    // convert attributes
    let attributes = Some(map_media_player_attributes(
        server,
        &entity_id,
        &state,
        Some(ha_attr),
    )?);

    Ok(AvailableIntgEntity {
        entity_id,
        device_id: None, // prepared for device_id handling
        entity_type: EntityType::MediaPlayer,
        device_class,
        name,
        features: Some(media_feats.into_iter().map(|v| v.to_string()).collect()),
        area: None,
        options: None,
        attributes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use url::Url;

    fn create_test_server() -> Url {
        Url::parse("http://homeassistant.local:8123").unwrap()
    }

    #[test]
    fn convert_media_player_basic() {
        let server = create_test_server();
        let entity_id = "media_player.living_room_tv".to_string();
        let state = "playing".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Living Room TV"
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id.clone(), state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(entity_id, entity.entity_id);
        assert_eq!(EntityType::MediaPlayer, entity.entity_type);
        assert_eq!(None, entity.device_class);
        assert_eq!(Some(&"Living Room TV".to_string()), entity.name.get("en"));
        assert!(entity.features.is_some());
        assert!(entity.attributes.is_some());
    }

    #[test]
    fn convert_media_player_no_friendly_name() {
        let server = create_test_server();
        let entity_id = "media_player.bedroom_speaker".to_string();
        let state = "idle".to_string();
        let mut ha_attr = serde_json::from_value(json!({})).unwrap();

        let result = convert_media_player_entity(&server, entity_id.clone(), state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(Some(&entity_id), entity.name.get("en"));
    }

    #[test]
    fn convert_media_player_with_device_class() {
        let server = create_test_server();
        let entity_id = "media_player.spotify".to_string();
        let state = "paused".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Spotify",
            "device_class": "speaker"
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(Some("speaker".to_string()), entity.device_class);
    }

    #[test]
    fn convert_media_player_invalid_device_class() {
        let server = create_test_server();
        let entity_id = "media_player.test".to_string();
        let state = "off".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "device_class": "invalid_class"
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(None, entity.device_class);
    }

    #[test]
    fn convert_media_player_all_features() {
        let server = create_test_server();
        let entity_id = "media_player.full_featured".to_string();
        let state = "playing".to_string();
        let supported_features = SUPPORT_TURN_ON
            | SUPPORT_TURN_OFF
            | SUPPORT_VOLUME_SET
            | SUPPORT_VOLUME_STEP
            | SUPPORT_SELECT_SOURCE
            | SUPPORT_VOLUME_MUTE
            | SUPPORT_PAUSE
            | SUPPORT_PLAY
            | SUPPORT_STOP
            | SUPPORT_NEXT_TRACK
            | SUPPORT_PREVIOUS_TRACK
            | SUPPORT_REPEAT_SET
            | SUPPORT_SHUFFLE_SET
            | SUPPORT_SELECT_SOUND_MODE
            | SUPPORT_SEEK;

        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Full Featured Player",
            "supported_features": supported_features
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Check that all expected features are present
        assert!(features.contains(&MediaPlayerFeature::OnOff.to_string()));
        assert!(features.contains(&MediaPlayerFeature::Volume.to_string()));
        assert!(features.contains(&MediaPlayerFeature::VolumeUpDown.to_string()));
        assert!(features.contains(&MediaPlayerFeature::SelectSource.to_string()));
        assert!(features.contains(&MediaPlayerFeature::Mute.to_string()));
        assert!(features.contains(&MediaPlayerFeature::Unmute.to_string()));
        assert!(features.contains(&MediaPlayerFeature::PlayPause.to_string()));
        assert!(features.contains(&MediaPlayerFeature::Stop.to_string()));
        assert!(features.contains(&MediaPlayerFeature::Next.to_string()));
        assert!(features.contains(&MediaPlayerFeature::Previous.to_string()));
        assert!(features.contains(&MediaPlayerFeature::Repeat.to_string()));
        assert!(features.contains(&MediaPlayerFeature::Shuffle.to_string()));
        assert!(features.contains(&MediaPlayerFeature::SelectSoundMode.to_string()));
        assert!(features.contains(&MediaPlayerFeature::Seek.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaDuration.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaPosition.to_string()));

        // Always present features
        assert!(features.contains(&MediaPlayerFeature::MediaTitle.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaArtist.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaAlbum.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaImageUrl.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaType.to_string()));
    }

    #[test]
    fn convert_media_player_no_features() {
        let server = create_test_server();
        let entity_id = "media_player.basic".to_string();
        let state = "off".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Basic Player",
            "supported_features": 0
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Only the always-present features should be there
        assert_eq!(5, features.len());
        assert!(features.contains(&MediaPlayerFeature::MediaTitle.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaArtist.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaAlbum.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaImageUrl.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaType.to_string()));
    }

    #[test]
    fn convert_media_player_partial_features() {
        let server = create_test_server();
        let entity_id = "media_player.partial".to_string();
        let state = "playing".to_string();
        let supported_features = SUPPORT_VOLUME_SET | SUPPORT_PLAY | SUPPORT_PAUSE;

        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Partial Player",
            "supported_features": supported_features
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(features.contains(&MediaPlayerFeature::Volume.to_string()));
        assert!(features.contains(&MediaPlayerFeature::PlayPause.to_string()));
        assert!(!features.contains(&MediaPlayerFeature::OnOff.to_string()));
        assert!(!features.contains(&MediaPlayerFeature::Stop.to_string()));
    }

    #[test]
    fn convert_media_player_mute_feature() {
        let server = create_test_server();
        let entity_id = "media_player.mute_test".to_string();
        let state = "playing".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "supported_features": SUPPORT_VOLUME_MUTE
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Both mute and unmute should be present
        assert!(features.contains(&MediaPlayerFeature::Mute.to_string()));
        assert!(features.contains(&MediaPlayerFeature::Unmute.to_string()));
    }

    #[test]
    fn convert_media_player_seek_feature() {
        let server = create_test_server();
        let entity_id = "media_player.seek_test".to_string();
        let state = "playing".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "supported_features": SUPPORT_SEEK
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Seek feature should add seek, duration, and position
        assert!(features.contains(&MediaPlayerFeature::Seek.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaDuration.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaPosition.to_string()));
    }

    #[test]
    fn convert_media_player_turn_on_off_combined() {
        let server = create_test_server();
        let entity_id = "media_player.onoff_test".to_string();
        let state = "off".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "supported_features": SUPPORT_TURN_ON | SUPPORT_TURN_OFF
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(features.contains(&MediaPlayerFeature::OnOff.to_string()));
    }

    #[test]
    fn convert_media_player_turn_on_only() {
        let server = create_test_server();
        let entity_id = "media_player.on_only_test".to_string();
        let state = "off".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "supported_features": SUPPORT_TURN_ON
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(features.contains(&MediaPlayerFeature::OnOff.to_string()));
    }

    #[test]
    fn convert_media_player_with_attributes() {
        let server = create_test_server();
        let entity_id = "media_player.with_attrs".to_string();
        let state = "playing".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Attributes Player",
            "volume_level": 0.5,
            "is_volume_muted": false,
            "media_title": "Test Song",
            "media_artist": "Test Artist"
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert!(entity.attributes.is_some());

        let attributes = entity.attributes.unwrap();
        assert_eq!(Some(&json!("PLAYING")), attributes.get("state"));
        assert_eq!(Some(&json!(50)), attributes.get("volume"));
        assert_eq!(Some(&json!(false)), attributes.get("muted"));
        assert_eq!(Some(&json!("Test Song")), attributes.get("media_title"));
        assert_eq!(Some(&json!("Test Artist")), attributes.get("media_artist"));
    }

    #[test]
    fn convert_media_player_missing_supported_features() {
        let server = create_test_server();
        let entity_id = "media_player.no_features".to_string();
        let state = "off".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "No Features Player"
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Should only have the always-present features
        assert_eq!(5, features.len());
        assert!(features.contains(&MediaPlayerFeature::MediaTitle.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaArtist.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaAlbum.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaImageUrl.to_string()));
        assert!(features.contains(&MediaPlayerFeature::MediaType.to_string()));
    }

    #[test]
    fn convert_media_player_invalid_supported_features() {
        let server = create_test_server();
        let entity_id = "media_player.invalid_features".to_string();
        let state = "off".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Invalid Features Player",
            "supported_features": "not_a_number"
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Should default to 0 and only have always-present features
        assert_eq!(5, features.len());
    }

    #[test]
    fn convert_media_player_all_states() {
        let server = create_test_server();
        let states = vec![
            "playing",
            "paused",
            "standby",
            "buffering",
            "idle",
            "off",
            "on",
            "unknown",
        ];

        for state in states {
            let entity_id = format!("media_player.state_test_{}", state);
            let mut ha_attr = serde_json::from_value(json!({
                "friendly_name": format!("State Test {}", state)
            }))
            .unwrap();

            let result =
                convert_media_player_entity(&server, entity_id, state.to_string(), &mut ha_attr);

            assert!(result.is_ok(), "Failed for state: {}", state);
            let entity = result.unwrap();
            assert!(entity.attributes.is_some());
        }
    }

    #[test]
    fn convert_media_player_entity_structure() {
        let server = create_test_server();
        let entity_id = "media_player.structure_test".to_string();
        let state = "playing".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Structure Test"
        }))
        .unwrap();

        let result = convert_media_player_entity(&server, entity_id.clone(), state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();

        // Verify required fields
        assert_eq!(entity_id, entity.entity_id);
        assert_eq!(None, entity.device_id);
        assert_eq!(EntityType::MediaPlayer, entity.entity_type);
        assert_eq!(None, entity.area);
        assert_eq!(None, entity.options);
        assert!(entity.name.contains_key("en"));
        assert!(entity.features.is_some());
        assert!(entity.attributes.is_some());
    }
}
