// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Cover entity specific logic.

use crate::client::event::convert_ha_onoff_state;
use crate::client::model::EventData;
use crate::errors::ServiceError;
use serde_json::{Map, Value};
use std::collections::HashMap;
use uc_api::intg::{AvailableIntgEntity, EntityChange};
use uc_api::{CoverFeature, EntityType};

// https://developers.home-assistant.io/docs/core/entity/cover#supported-features
pub const COVER_SUPPORT_OPEN: u32 = 1;
pub const COVER_SUPPORT_CLOSE: u32 = 2;
pub const COVER_SUPPORT_SET_POSITION: u32 = 4;
pub const COVER_SUPPORT_STOP: u32 = 8;
// pub const COVER_SUPPORT_OPEN_TILT: u32 = 16;
// pub const COVER_SUPPORT_CLOSE_TILT: u32 = 32;
// pub const COVER_SUPPORT_STOP_TILT: u32 = 64;
// pub const COVER_SUPPORT_SET_TILT_POSITION: u32 = 128;

pub(crate) fn map_cover_attributes(
    _entity_id: &str,
    state: &str,
    ha_attr: Option<&mut Map<String, Value>>,
) -> Result<Map<String, Value>, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(3);

    let state = match state {
        "open" | "opening" | "closed" | "closing" => state.to_uppercase().into(),
        _ => convert_ha_onoff_state(state)?,
    };
    attributes.insert("state".into(), state);

    if let Some(ha_attr) = ha_attr {
        if let Some(value @ 0..=100) = ha_attr.get("current_position").and_then(|v| v.as_u64()) {
            attributes.insert("position".into(), value.into());
        }
        if let Some(value @ 0..=100) = ha_attr
            .get("current_tilt_position")
            .and_then(|v| v.as_u64())
        {
            attributes.insert("tilt_position".into(), value.into());
        }
    }

    Ok(attributes)
}

pub(crate) fn cover_event_to_entity_change(
    mut data: EventData,
) -> Result<EntityChange, ServiceError> {
    let attributes = map_cover_attributes(
        &data.entity_id,
        &data.new_state.state,
        data.new_state.attributes.as_mut(),
    )?;

    Ok(EntityChange {
        device_id: None,
        entity_type: EntityType::Cover,
        entity_id: data.entity_id,
        attributes,
    })
}

pub(crate) fn convert_cover_entity(
    entity_id: String,
    state: String,
    ha_attr: &mut Map<String, Value>,
) -> Result<AvailableIntgEntity, ServiceError> {
    let friendly_name = ha_attr.get("friendly_name").and_then(|v| v.as_str());
    let name = HashMap::from([("en".into(), friendly_name.unwrap_or(&entity_id).into())]);
    let device_class = ha_attr.get("device_class").and_then(|v| v.as_str());
    let device_class = match device_class {
        Some("blind") | Some("curtain") | Some("garage") | Some("shade") => {
            device_class.map(|v| v.into())
        }
        _ => None,
    };

    // handle features
    let supported_features = ha_attr
        .get("supported_features")
        .and_then(|v| v.as_u64())
        .unwrap_or_default() as u32;
    let mut cover_feats = Vec::with_capacity(2);

    if supported_features & COVER_SUPPORT_OPEN > 0 {
        cover_feats.push(CoverFeature::Open);
    }
    if supported_features & COVER_SUPPORT_CLOSE > 0 {
        cover_feats.push(CoverFeature::Close);
    }
    if supported_features & COVER_SUPPORT_STOP > 0 {
        cover_feats.push(CoverFeature::Stop);
    }
    if supported_features & COVER_SUPPORT_SET_POSITION > 0 {
        cover_feats.push(CoverFeature::Position);
    }

    // convert attributes
    let attributes = Some(map_cover_attributes(&entity_id, &state, Some(ha_attr))?);

    Ok(AvailableIntgEntity {
        entity_id,
        device_id: None, // TODO prepare device_id handling
        entity_type: EntityType::Cover,
        device_class,
        name,
        features: Some(cover_feats.into_iter().map(|v| v.to_string()).collect()),
        area: None,
        options: None,
        attributes,
    })
}
