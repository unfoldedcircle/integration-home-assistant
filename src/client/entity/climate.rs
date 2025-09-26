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
use uc_api::{ClimateFeature, ClimateOptionField, EntityType};

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

    // TODO not completely tested, need to test "cool"! #11
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

    // TODO is this the correct way to find out if the device can measure the current temperature? #12
    if is_float_value(ha_attr, "current_temperature") {
        climate_feats.push(ClimateFeature::CurrentTemperature);
    }

    // handle options. TODO untested! Only based on some GitHub issue logs :-) #12
    let mut options = serde_json::Map::new();
    if let Some(v) = number_value(ha_attr, "min_temp") {
        options.insert(ClimateOptionField::MinTemperature.to_string(), v);
    }
    if let Some(v) = number_value(ha_attr, "max_temp") {
        options.insert(ClimateOptionField::MaxTemperature.to_string(), v);
    }
    if let Some(v) = number_value(ha_attr, "target_temp_step") {
        options.insert(ClimateOptionField::TargetTemperatureStep.to_string(), v);
    }
    // TODO how do we get the HA temperature_unit attribute? Couldn't find an example... #10
    if let Some(v) = ha_attr.get("temperature_unit") {
        options.insert(ClimateOptionField::TemperatureUnit.to_string(), v.clone());
    }

    // convert attributes
    let attributes = Some(map_climate_attributes(&entity_id, &state, Some(ha_attr))?);

    Ok(AvailableIntgEntity {
        entity_id,
        device_id: None, // prepared for device_id handling
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
    use super::*;
    use crate::client::entity::climate_event_to_entity_change;
    use crate::client::model::EventData;
    use rstest::rstest;
    use serde_json::{Value, json};
    use uc_api::intg::EntityChange;
    use uc_api::{ClimateFeature, ClimateOptionField, EntityType};

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

    // Enhanced tests for convert_climate_entity function

    #[test]
    fn convert_climate_entity_basic() {
        let entity_id = "climate.living_room_thermostat".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Living Room Thermostat"
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id.clone(), state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(entity_id, entity.entity_id);
        assert_eq!(EntityType::Climate, entity.entity_type);
        assert_eq!(None, entity.device_class);
        assert_eq!(
            Some(&"Living Room Thermostat".to_string()),
            entity.name.get("en")
        );
        assert!(entity.features.is_some());
        assert!(entity.attributes.is_some());
    }

    #[test]
    fn convert_climate_entity_no_friendly_name() {
        let entity_id = "climate.bedroom_ac".to_string();
        let state = "cool".to_string();
        let mut ha_attr = serde_json::from_value(json!({})).unwrap();

        let result = convert_climate_entity(entity_id.clone(), state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(Some(&entity_id), entity.name.get("en"));
    }

    #[rstest]
    #[case("off")]
    #[case("heat")]
    #[case("cool")]
    #[case("heat_cool")]
    #[case("auto")]
    #[case("fan_only")]
    #[case("unavailable")]
    #[case("unknown")]
    fn convert_climate_entity_all_states(#[case] state: &str) {
        let entity_id = format!("climate.state_test_{}", state);
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": format!("State Test {}", state)
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state.to_string(), &mut ha_attr);

        assert!(result.is_ok(), "Failed for state: {}", state);
        let entity = result.unwrap();
        assert!(entity.attributes.is_some());

        let attributes = entity.attributes.unwrap();
        let expected_state = match state {
            "fan_only" => "FAN".to_string(),
            s => s.to_uppercase(),
        };
        if state != "unknown" {
            // unknown states get warnings but still work
            assert_eq!(Some(&json!(expected_state)), attributes.get("state"));
        }
    }

    #[test]
    fn convert_climate_entity_no_hvac_modes() {
        let entity_id = "climate.basic".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Basic Climate",
            "supported_features": 0
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Should have no HVAC mode features when hvac_modes is missing
        assert!(!features.contains(&ClimateFeature::OnOff.to_string()));
        assert!(!features.contains(&ClimateFeature::Heat.to_string()));
        assert!(!features.contains(&ClimateFeature::Cool.to_string()));
    }

    #[rstest]
    #[case("off", ClimateFeature::OnOff)]
    #[case("heat", ClimateFeature::Heat)]
    #[case("cool", ClimateFeature::Cool)]
    fn convert_climate_entity_hvac_mode_features(
        #[case] hvac_mode: &str,
        #[case] expected_feature: ClimateFeature,
    ) {
        let entity_id = format!("climate.{}_test", hvac_mode);
        let state = hvac_mode.to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": format!("{} Test", hvac_mode),
            "hvac_modes": [hvac_mode]
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Failed for HVAC mode: {}", hvac_mode);
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(features.contains(&expected_feature.to_string()));
    }

    #[test]
    fn convert_climate_entity_all_hvac_modes() {
        let entity_id = "climate.full_hvac".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Full HVAC System",
            "hvac_modes": ["off", "heat", "cool", "auto", "fan_only"]
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(features.contains(&ClimateFeature::OnOff.to_string()));
        assert!(features.contains(&ClimateFeature::Heat.to_string()));
        assert!(features.contains(&ClimateFeature::Cool.to_string()));
        // Note: fan_only and auto are not currently mapped to features
    }

    #[test]
    fn convert_climate_entity_with_target_temperature_feature() {
        let entity_id = "climate.target_temp".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Target Temperature Climate",
            "supported_features": SUPPORT_TARGET_TEMPERATURE
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(features.contains(&ClimateFeature::TargetTemperature.to_string()));
    }

    #[test]
    fn convert_climate_entity_with_current_temperature() {
        let entity_id = "climate.current_temp".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Current Temperature Climate",
            "current_temperature": 22.5
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(features.contains(&ClimateFeature::CurrentTemperature.to_string()));
    }

    #[test]
    fn convert_climate_entity_with_all_features() {
        let entity_id = "climate.full_featured".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Full Featured Climate",
            "hvac_modes": ["off", "heat", "cool"],
            "supported_features": SUPPORT_TARGET_TEMPERATURE,
            "current_temperature": 22.0
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(features.contains(&ClimateFeature::OnOff.to_string()));
        assert!(features.contains(&ClimateFeature::Heat.to_string()));
        assert!(features.contains(&ClimateFeature::Cool.to_string()));
        assert!(features.contains(&ClimateFeature::TargetTemperature.to_string()));
        assert!(features.contains(&ClimateFeature::CurrentTemperature.to_string()));
    }

    #[test]
    fn convert_climate_entity_no_options() {
        let entity_id = "climate.no_options".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "No Options Climate"
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();

        // Should have no options when temperature limits are not provided
        assert_eq!(None, entity.options);
    }

    #[test]
    fn convert_climate_entity_with_temperature_options() {
        let entity_id = "climate.with_options".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Options Climate",
            "min_temp": 10,
            "max_temp": 30,
            "target_temp_step": 0.5,
            "temperature_unit": "°C"
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert!(entity.options.is_some());

        let options = entity.options.unwrap();
        assert_eq!(
            Some(&json!(10)),
            options.get(&ClimateOptionField::MinTemperature.to_string())
        );
        assert_eq!(
            Some(&json!(30)),
            options.get(&ClimateOptionField::MaxTemperature.to_string())
        );
        assert_eq!(
            Some(&json!(0.5)),
            options.get(&ClimateOptionField::TargetTemperatureStep.to_string())
        );
        assert_eq!(
            Some(&json!("°C")),
            options.get(&ClimateOptionField::TemperatureUnit.to_string())
        );
    }

    #[test]
    fn convert_climate_entity_partial_options() {
        let entity_id = "climate.partial_options".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Partial Options Climate",
            "min_temp": 15,
            "max_temp": 25
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert!(entity.options.is_some());

        let options = entity.options.unwrap();
        assert_eq!(2, options.len());
        assert_eq!(
            Some(&json!(15)),
            options.get(&ClimateOptionField::MinTemperature.to_string())
        );
        assert_eq!(
            Some(&json!(25)),
            options.get(&ClimateOptionField::MaxTemperature.to_string())
        );
        assert!(
            options
                .get(&ClimateOptionField::TargetTemperatureStep.to_string())
                .is_none()
        );
        assert!(
            options
                .get(&ClimateOptionField::TemperatureUnit.to_string())
                .is_none()
        );
    }

    #[test]
    fn convert_climate_entity_structure() {
        let entity_id = "climate.structure_test".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Structure Test Climate"
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id.clone(), state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();

        // Verify required fields
        assert_eq!(entity_id, entity.entity_id);
        assert_eq!(None, entity.device_id);
        assert_eq!(EntityType::Climate, entity.entity_type);
        assert_eq!(None, entity.device_class);
        assert_eq!(None, entity.area);
        assert!(entity.name.contains_key("en"));
        assert!(entity.features.is_some());
        assert!(entity.attributes.is_some());
    }

    #[test]
    fn convert_climate_entity_with_attributes() {
        let entity_id = "climate.with_attrs".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Attributes Climate",
            "current_temperature": 21.5,
            "temperature": 24.0,
            "target_temperature_high": 26.0,
            "target_temperature_low": 18.0,
            "fan_mode": "auto"
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert!(entity.attributes.is_some());

        let attributes = entity.attributes.unwrap();
        assert_eq!(Some(&json!("HEAT")), attributes.get("state"));
        assert_eq!(Some(&json!(21.5)), attributes.get("current_temperature"));
        assert_eq!(Some(&json!(24.0)), attributes.get("target_temperature"));
        assert_eq!(
            Some(&json!(26.0)),
            attributes.get("target_temperature_high")
        );
        assert_eq!(Some(&json!(18.0)), attributes.get("target_temperature_low"));
        assert_eq!(Some(&json!("AUTO")), attributes.get("fan_mode"));
    }

    #[test]
    fn convert_climate_entity_missing_supported_features() {
        let entity_id = "climate.no_features".to_string();
        let state = "off".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "No Features Climate"
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Should not have target temperature feature when supported_features is missing
        assert!(!features.contains(&ClimateFeature::TargetTemperature.to_string()));
    }

    #[test]
    fn convert_climate_entity_invalid_supported_features() {
        let entity_id = "climate.invalid_features".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Invalid Features Climate",
            "supported_features": "not_a_number"
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Should default to 0 and not have target temperature feature
        assert!(!features.contains(&ClimateFeature::TargetTemperature.to_string()));
    }

    #[test]
    fn convert_climate_entity_invalid_hvac_modes() {
        let entity_id = "climate.invalid_hvac".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Invalid HVAC Modes Climate",
            "hvac_modes": ["unknown_mode", "invalid", "heat"]
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Should only have heat feature, ignoring invalid modes
        assert!(features.contains(&ClimateFeature::Heat.to_string()));
        assert!(!features.contains(&ClimateFeature::OnOff.to_string()));
        assert!(!features.contains(&ClimateFeature::Cool.to_string()));
    }

    #[test]
    fn convert_climate_entity_current_temperature_not_float() {
        let entity_id = "climate.non_float_temp".to_string();
        let state = "heat".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Non Float Temperature Climate",
            "current_temperature": "not_a_number"
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(!features.contains(&ClimateFeature::CurrentTemperature.to_string()));
    }

    #[test]
    fn convert_climate_entity_with_target_temperature_range_feature() {
        let entity_id = "climate.temp_range".to_string();
        let state = "heat_cool".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Temperature Range Climate",
            "supported_features": SUPPORT_TARGET_TEMPERATURE_RANGE
        }))
        .unwrap();

        let result = convert_climate_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(!features.contains(&"TargetTemperatureRange".to_string()));
    }
}
