// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Climate entity specific HA event logic.

use crate::client::model::EventData;
use crate::errors::ServiceError;
use crate::util::json;
use log::{debug, warn};
use uc_api::intg::EntityChange;
use uc_api::EntityType;

pub(crate) fn climate_event_to_entity_change(
    data: EventData,
) -> Result<EntityChange, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(6);

    // TODO are there other states which need to be handled?
    // We got hvac_mode states in `state` in YIO v1. Entity docs don't mention them though...
    match data.new_state.state.as_str() {
        "unavailable" | "unknown" => {
            attributes.insert("state".into(), data.new_state.state.to_uppercase().into());
        }
        _ => debug!("{} climate state: {}", data.entity_id, data.new_state.state),
    };

    if let Some(mut ha_attr) = data.new_state.attributes {
        let mode = ha_attr
            .get("hvac_mode")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        match mode {
            "off" | "heat" | "cool" | "heat_cool" | "auto" => {
                attributes.insert("state".into(), mode.to_uppercase().into());
            }
            "fan_only" => {
                attributes.insert("state".into(), "FAN".into());
            }
            "" => {} // attribute not present
            _ => {
                warn!("Not supported hvac_mode: '{}'", mode);
            }
        }
        json::move_entry(&mut ha_attr, &mut attributes, "current_temperature");
        json::move_value(
            &mut ha_attr,
            &mut attributes,
            "temperature",
            "target_temperature",
        );
        json::move_entry(&mut ha_attr, &mut attributes, "target_temperature_high");
        json::move_entry(&mut ha_attr, &mut attributes, "target_temperature_low");
        if let Some(value) = ha_attr.get("fan_mode").and_then(|v| v.as_str()) {
            // TODO test and filter fan modes?
            attributes.insert("fan_mode".into(), value.to_uppercase().into());
        }
    }

    Ok(EntityChange {
        device_id: None,
        entity_type: EntityType::Climate,
        entity_id: data.entity_id,
        attributes,
    })
}
