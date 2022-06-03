// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Climate entity specific logic.

use crate::client::model::EventData;
use crate::errors::ServiceError;
use crate::util::json;
use crate::util::json::{is_float_value, number_value};
use log::warn;
use serde_json::{Map, Value};
use std::collections::HashMap;
use uc_api::intg::{AvailableIntgEntity, EntityChange};
use uc_api::{ClimateFeature, ClimateOption, EntityType};

// https://developers.home-assistant.io/docs/core/entity/climate#supported-features
pub const SUPPORT_TARGET_TEMPERATURE: u32 = 1;
pub const SUPPORT_TARGET_TEMPERATURE_RANGE: u32 = 2;
/* not yet used constants
pub const SUPPORT_TARGET_HUMIDITY: u32 = 4;
pub const SUPPORT_FAN_MODE: u32 = 8;
pub const SUPPORT_PRESET_MODE: u32 = 16;
pub const SUPPORT_SWING_MODE: u32 = 32;
pub const SUPPORT_AUX_HEAT: u32 = 64;
*/

pub(crate) fn map_climate_attributes(
    entity_id: &str,
    state: &str,
    ha_attr: Option<&mut Map<String, Value>>,
) -> Result<Map<String, Value>, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(6);

    match state {
        // general states
        "unavailable" | "unknown" |
        // hvac states
        "off" | "heat" | "cool" | "heat_cool" | "auto" => {
            attributes.insert("state".into(), state.to_uppercase().into());
        }
        "fan_only" => {
            attributes.insert("state".into(), "FAN".into());
        }
        state => warn!("{} Not supported climate state: {}", entity_id, state),
    };

    if let Some(ha_attr) = ha_attr {
        json::move_entry(ha_attr, &mut attributes, "current_temperature");
        // TODO temperature value might be null! Filter or leave it?
        json::move_value(
            ha_attr,
            &mut attributes,
            "temperature",
            "target_temperature",
        );
        json::move_entry(ha_attr, &mut attributes, "target_temperature_high");
        json::move_entry(ha_attr, &mut attributes, "target_temperature_low");
        if let Some(value) = ha_attr.get("fan_mode").and_then(|v| v.as_str()) {
            // TODO test and filter fan modes?
            attributes.insert("fan_mode".into(), value.to_uppercase().into());
        }
    }

    Ok(attributes)
}

pub(crate) fn climate_event_to_entity_change(
    mut data: EventData,
) -> Result<EntityChange, ServiceError> {
    let attributes = map_climate_attributes(
        &data.entity_id,
        &data.new_state.state,
        data.new_state.attributes.as_mut(),
    )?;

    Ok(EntityChange {
        device_id: None,
        entity_type: EntityType::Climate,
        entity_id: data.entity_id,
        attributes,
    })
}

pub(crate) fn convert_climate_entity(
    entity_id: String,
    state: String,
    ha_attr: &mut Map<String, Value>,
) -> Result<AvailableIntgEntity, ServiceError> {
    let friendly_name = ha_attr.get("friendly_name").and_then(|v| v.as_str());
    let name = HashMap::from([("en".into(), friendly_name.unwrap_or(&entity_id).into())]);

    // handle features
    let supported_features = ha_attr
        .get("supported_features")
        .and_then(|v| v.as_u64())
        .unwrap_or_default() as u32;
    let mut climate_feats = Vec::with_capacity(4);

    // FIXME this only a theoretical implementation and completely untested!
    // https://developers.home-assistant.io/docs/core/entity/climate#hvac-modes
    // Need to find some real climate devices to test...
    if let Some(hvac_modes) = ha_attr.get("hvac_modes").and_then(|v| v.as_array()) {
        for hvac_mode in hvac_modes {
            let feature = match hvac_mode.as_str().unwrap_or_default() {
                "off" => ClimateFeature::OnOff,
                "heat" => ClimateFeature::Heat,
                "cool" => ClimateFeature::Cool,
                // "fan_only" => ClimateFeature::Fan,
                &_ => continue,
            };
            climate_feats.push(feature);
        }
    }

    if supported_features & SUPPORT_TARGET_TEMPERATURE > 0 {
        climate_feats.push(ClimateFeature::TargetTemperature);
    }
    if supported_features & SUPPORT_TARGET_TEMPERATURE_RANGE > 0 {
        /* sorry, not yet implemented
            climate_feats.push(ClimateFeature::TargetTemperatureRange)
        */
    }

    // TODO is this the correct way to find out if the device can measure the current temperature?
    if is_float_value(ha_attr, "current_temperature") {
        climate_feats.push(ClimateFeature::CurrentTemperature);
    }

    // handle options. TODO untested! Only based on some GitHub issue logs :-)
    let mut options = serde_json::Map::new();
    if let Some(v) = number_value(ha_attr, "min_temp") {
        options.insert(ClimateOption::MinTemperature.to_string(), v);
    }
    if let Some(v) = number_value(ha_attr, "max_temp") {
        options.insert(ClimateOption::MaxTemperature.to_string(), v);
    }
    if let Some(v) = number_value(ha_attr, "target_temp_step") {
        options.insert(ClimateOption::TargetTemperatureStep.to_string(), v);
    }
    // TODO how do we get the HA temperature_unit attribute? Couldn't find an example...
    if let Some(v) = ha_attr.get("temperature_unit") {
        options.insert(ClimateOption::TemperatureUnit.to_string(), v.clone());
    }

    // convert attributes
    let attributes = Some(map_climate_attributes(&entity_id, &state, Some(ha_attr))?);

    Ok(AvailableIntgEntity {
        entity_id,
        device_id: None, // TODO prepare device_id handling
        entity_type: EntityType::Climate,
        device_class: None,
        name,
        features: Some(climate_feats.into_iter().map(|v| v.to_string()).collect()),
        area: None,
        options: if options.is_empty() {
            None
        } else {
            Some(options)
        },
        attributes,
    })
}

#[cfg(test)]
mod tests {
    use crate::client::entity::climate_event_to_entity_change;
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
