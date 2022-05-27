// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Climate entity specific HA event logic.

use crate::client::model::EventData;
use crate::errors::ServiceError;
use crate::util::json;
use log::warn;
use uc_api::intg::EntityChange;
use uc_api::EntityType;

pub(crate) fn climate_event_to_entity_change(
    data: EventData,
) -> Result<EntityChange, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(6);

    match data.new_state.state.as_str() {
        // general states
        "unavailable" | "unknown" |
        // hvac states
        "off" | "heat" | "cool" | "heat_cool" | "auto" => {
            attributes.insert("state".into(), data.new_state.state.to_uppercase().into());
        }
        "fan_only" => {
            attributes.insert("state".into(), "FAN".into());
        }
        state => warn!("{} Not supported climate state: {}", data.entity_id, state),
    };

    if let Some(mut ha_attr) = data.new_state.attributes {
        json::move_entry(&mut ha_attr, &mut attributes, "current_temperature");
        // TODO temperature value might be null! Filter or leave it?
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

#[cfg(test)]
mod tests {
    use crate::client::event::climate::climate_event_to_entity_change;
    use crate::client::model::EventData;
    use serde_json::{json, Value};
    use uc_api::intg::EntityChange;
    use uc_api::EntityType;

    #[test]
    fn climate_event_heat() {
        let new_state = json!({
            "entity_id": "climate.bathroom_floor_heating_mode",
            "state": "heat",
            "attributes": {
                "hvac_modes": [
                    "off",
                    "heat",
                    "cool"
                ],
                "min_temp": 5,
                "max_temp": 40,
                "preset_modes": [
                    "none",
                    "Energy heat"
                ],
                "current_temperature": 22.6,
                "temperature": 29.5,
                "preset_mode": "none",
                "friendly_name": "Bathroom floor heating",
                "supported_features": 17
            }
        });
        let event = map_new_state(new_state);

        assert_eq!(Some(&json!("HEAT")), event.attributes.get("state"));
        assert_eq!(
            Some(&json!(22.6)),
            event.attributes.get("current_temperature")
        );
        assert_eq!(
            Some(&json!(29.5)),
            event.attributes.get("target_temperature")
        );
    }

    #[test]
    fn climate_event_off() {
        let new_state = json!({
            "entity_id": "climate.bathroom_floor_heating_mode",
            "state": "off",
            "attributes": {
                "hvac_modes": [
                    "off",
                    "heat",
                    "cool"
                ],
                "min_temp": 7,
                "max_temp": 35,
                "preset_modes": [
                    "none",
                    "Energy heat"
                ],
                "current_temperature": 22.6,
                "temperature": null,
                "preset_mode": "none",
                "friendly_name": "Bathroom floor heating",
                "supported_features": 17
            }
        });
        let event = map_new_state(new_state);

        assert_eq!(Some(&json!("OFF")), event.attributes.get("state"));
        assert_eq!(
            Some(&json!(22.6)),
            event.attributes.get("current_temperature")
        );
        assert_eq!(
            Some(&Value::Null),
            event.attributes.get("target_temperature")
        );
    }

    fn map_new_state(new_state: Value) -> EntityChange {
        let data = EventData {
            entity_id: "test".into(),
            new_state: serde_json::from_value(new_state).expect("invalid test data"),
        };
        let result = climate_event_to_entity_change(data);
        assert!(
            result.is_ok(),
            "Expected successful event mapping but got: {:?}",
            result.unwrap_err()
        );
        let entity_change = result.unwrap();
        assert_eq!(EntityType::Climate, entity_change.entity_type);

        entity_change
    }
}
