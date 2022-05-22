// Copyright (c) 2022 {person OR org} <{email}>
// SPDX-License-Identifier: MPL-2.0

//! Media player entity specific HA event logic.

use crate::client::event;
use crate::client::event::convert_ha_onoff_state;
use crate::client::model::EventData;
use crate::errors::ServiceError;
use log::error;
use uc_api::intg::EntityChange;
use uc_api::EntityType;
use url::Url;

pub(crate) fn media_player_event_to_entity_change(
    server: &Url,
    data: EventData,
) -> Result<EntityChange, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(8);

    let state = match data.new_state.state.as_str() {
        "playing" | "paused" => data.new_state.state.to_uppercase().into(),
        _ => convert_ha_onoff_state(&data.new_state.state)?,
    };
    attributes.insert("state".into(), state);

    if let Some(mut ha_attr) = data.new_state.attributes {
        if let Some(value) = ha_attr.get("volume_level").and_then(|v| v.as_f64()) {
            attributes.insert("volume".into(), ((value * 100.0).round() as u64).into());
        }
        event::move_json_value(&mut ha_attr, &mut attributes, "is_volume_muted", "muted");
        event::move_json_attribute(&mut ha_attr, &mut attributes, "media_position");
        event::move_json_attribute(&mut ha_attr, &mut attributes, "media_duration");
        event::move_json_attribute(&mut ha_attr, &mut attributes, "media_title");
        event::move_json_attribute(&mut ha_attr, &mut attributes, "media_artist");
        event::move_json_value(
            &mut ha_attr,
            &mut attributes,
            "media_album_name",
            "media_album",
        );
        event::move_json_value(
            &mut ha_attr,
            &mut attributes,
            "media_content_type",
            "media_type",
        );
        event::move_json_attribute(&mut ha_attr, &mut attributes, "shuffle");
        if let Some(value) = ha_attr.get("repeat").and_then(|v| v.as_str()) {
            attributes.insert("repeat".into(), value.to_uppercase().into());
        }
        event::move_json_attribute(&mut ha_attr, &mut attributes, "source");
        event::move_json_attribute(&mut ha_attr, &mut attributes, "sound_mode");

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

    Ok(EntityChange {
        device_id: None,
        entity_type: EntityType::MediaPlayer,
        entity_id: data.entity_id,
        attributes,
    })
}
