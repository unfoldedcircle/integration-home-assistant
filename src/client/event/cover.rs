// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Cover entity specific HA event logic.

use crate::client::event::convert_ha_onoff_state;
use crate::client::model::EventData;
use crate::errors::ServiceError;
use uc_api::intg::EntityChange;
use uc_api::EntityType;

pub(crate) fn cover_event_to_entity_change(data: EventData) -> Result<EntityChange, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(3);

    let state = match data.new_state.state.as_str() {
        "open" | "opening" | "closed" | "closing" => data.new_state.state.to_uppercase().into(),
        _ => convert_ha_onoff_state(&data.new_state.state)?,
    };
    attributes.insert("state".into(), state);

    if let Some(ha_attr) = data.new_state.attributes {
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

    Ok(EntityChange {
        device_id: None,
        entity_type: EntityType::Cover,
        entity_id: data.entity_id,
        attributes,
    })
}
