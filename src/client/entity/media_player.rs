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
use uc_api::{EntityType, MediaPlayerFeature};
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
// pub const SUPPORT_SELECT_SOURCE: u32 = 2048;
pub const SUPPORT_STOP: u32 = 4096;
// pub const SUPPORT_CLEAR_PLAYLIST: u32 = 8192;
pub const SUPPORT_PLAY: u32 = 16384;
pub const SUPPORT_SHUFFLE_SET: u32 = 32768;
// pub const SUPPORT_SELECT_SOUND_MODE: u32 = 65536;
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
        "playing" | "paused" => state.to_uppercase().into(),
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
        json::move_entry(ha_attr, &mut attributes, "media_duration");
        json::move_entry(ha_attr, &mut attributes, "media_title");
        json::move_entry(ha_attr, &mut attributes, "media_artist");
        json::move_value(ha_attr, &mut attributes, "media_album_name", "media_album");
        json::move_value(ha_attr, &mut attributes, "media_content_type", "media_type");
        json::move_entry(ha_attr, &mut attributes, "shuffle");
        if let Some(value) = ha_attr.get("repeat").and_then(|v| v.as_str()) {
            attributes.insert("repeat".into(), value.to_uppercase().into());
        }
        json::move_entry(ha_attr, &mut attributes, "source");
        json::move_entry(ha_attr, &mut attributes, "sound_mode");

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
                error!("Unexpected entity_picture format: {}", value);
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
    let device_class = ha_attr.get("device_class").and_then(|v| v.as_str());
    let device_class = match device_class {
        Some("receiver") | Some("speaker") => device_class.map(|v| v.into()),
        _ => None,
    };

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

    if supported_features & SUPPORT_SELECT_SOURCE > 0 {
        features.push("SOURCE");
    }
     */

    // TODO media_player entity options: volume_steps - do we get that from HASS?

    // convert attributes
    let attributes = Some(map_media_player_attributes(
        server,
        &entity_id,
        &state,
        Some(ha_attr),
    )?);

    Ok(AvailableIntgEntity {
        entity_id,
        device_id: None, // TODO prepare device_id handling
        entity_type: EntityType::MediaPlayer,
        device_class,
        name,
        features: Some(media_feats.into_iter().map(|v| v.to_string()).collect()),
        area: None,
        options: None,
        attributes,
    })
}
