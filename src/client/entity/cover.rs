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
        device_id: None, // prepared for device_id handling
        entity_type: EntityType::Cover,
        device_class,
        name,
        features: Some(cover_feats.into_iter().map(|v| v.to_string()).collect()),
        area: None,
        options: None,
        attributes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use serde_json::json;
    use uc_api::{CoverFeature, EntityType};

    #[test]
    fn convert_cover_entity_basic() {
        let entity_id = "cover.living_room_blinds".to_string();
        let state = "open".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Living Room Blinds"
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id.clone(), state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(entity_id, entity.entity_id);
        assert_eq!(EntityType::Cover, entity.entity_type);
        assert_eq!(None, entity.device_class);
        assert_eq!(
            Some(&"Living Room Blinds".to_string()),
            entity.name.get("en")
        );
        assert!(entity.features.is_some());
        assert!(entity.attributes.is_some());
    }

    #[test]
    fn convert_cover_entity_no_friendly_name() {
        let entity_id = "cover.bedroom_curtains".to_string();
        let state = "closed".to_string();
        let mut ha_attr = serde_json::from_value(json!({})).unwrap();

        let result = convert_cover_entity(entity_id.clone(), state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(Some(&entity_id), entity.name.get("en"));
    }

    #[rstest]
    #[case("blind")]
    #[case("curtain")]
    #[case("garage")]
    #[case("shade")]
    fn convert_cover_entity_with_supported_device_classes(#[case] device_class: &str) {
        let entity_id = format!("cover.{}_{}", device_class, "test");
        let state = "open".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": format!("Test {}", device_class),
            "device_class": device_class
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Failed for device class: {}", device_class);
        let entity = result.unwrap();
        assert_eq!(Some(device_class.to_string()), entity.device_class);
    }

    #[rstest]
    #[case("window")]
    #[case("door")]
    #[case("invalid_class")]
    #[case("")]
    fn convert_cover_entity_with_unsupported_device_classes(#[case] device_class: &str) {
        let entity_id = "cover.unsupported_test".to_string();
        let state = "open".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Unsupported Test",
            "device_class": device_class
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(
            result.is_ok(),
            "Failed for unsupported device class: {}",
            device_class
        );
        let entity = result.unwrap();
        assert_eq!(None, entity.device_class);
    }

    #[rstest]
    #[case("open")]
    #[case("opening")]
    #[case("closed")]
    #[case("closing")]
    #[case("unavailable")]
    #[case("unknown")]
    fn convert_cover_entity_all_states(#[case] state: &str) {
        let entity_id = format!("cover.state_test_{}", state);
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": format!("State Test {}", state)
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state.to_string(), &mut ha_attr);

        assert!(result.is_ok(), "Failed for state: {}", state);
        let entity = result.unwrap();
        assert!(entity.attributes.is_some());

        let attributes = entity.attributes.unwrap();
        match state {
            "open" => assert_eq!(Some(&json!("OPEN")), attributes.get("state")),
            "opening" => assert_eq!(Some(&json!("OPENING")), attributes.get("state")),
            "closed" => assert_eq!(Some(&json!("CLOSED")), attributes.get("state")),
            "closing" => assert_eq!(Some(&json!("CLOSING")), attributes.get("state")),
            "unavailable" => assert_eq!(Some(&json!("UNAVAILABLE")), attributes.get("state")),
            "unknown" => assert_eq!(Some(&json!("UNKNOWN")), attributes.get("state")),
            _ => {}
        }
    }

    #[test]
    fn convert_cover_entity_no_features() {
        let entity_id = "cover.basic".to_string();
        let state = "open".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Basic Cover",
            "supported_features": 0
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Should have no features
        assert_eq!(0, features.len());
    }

    #[test]
    fn convert_cover_entity_with_open_feature() {
        let entity_id = "cover.open_only".to_string();
        let state = "closed".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Open Only Cover",
            "supported_features": COVER_SUPPORT_OPEN
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert_eq!(1, features.len());
        assert!(features.contains(&CoverFeature::Open.to_string()));
        assert!(!features.contains(&CoverFeature::Close.to_string()));
    }

    #[test]
    fn convert_cover_entity_with_close_feature() {
        let entity_id = "cover.close_only".to_string();
        let state = "open".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Close Only Cover",
            "supported_features": COVER_SUPPORT_CLOSE
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert_eq!(1, features.len());
        assert!(features.contains(&CoverFeature::Close.to_string()));
        assert!(!features.contains(&CoverFeature::Open.to_string()));
    }

    #[test]
    fn convert_cover_entity_with_stop_feature() {
        let entity_id = "cover.stop_only".to_string();
        let state = "opening".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Stop Only Cover",
            "supported_features": COVER_SUPPORT_STOP
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert_eq!(1, features.len());
        assert!(features.contains(&CoverFeature::Stop.to_string()));
    }

    #[test]
    fn convert_cover_entity_with_position_feature() {
        let entity_id = "cover.position_only".to_string();
        let state = "open".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Position Only Cover",
            "supported_features": COVER_SUPPORT_SET_POSITION
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert_eq!(1, features.len());
        assert!(features.contains(&CoverFeature::Position.to_string()));
    }

    #[test]
    fn convert_cover_entity_with_all_features() {
        let entity_id = "cover.full_featured".to_string();
        let state = "open".to_string();
        let supported_features = COVER_SUPPORT_OPEN
            | COVER_SUPPORT_CLOSE
            | COVER_SUPPORT_STOP
            | COVER_SUPPORT_SET_POSITION;

        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Full Featured Cover",
            "supported_features": supported_features
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert_eq!(4, features.len());
        assert!(features.contains(&CoverFeature::Open.to_string()));
        assert!(features.contains(&CoverFeature::Close.to_string()));
        assert!(features.contains(&CoverFeature::Stop.to_string()));
        assert!(features.contains(&CoverFeature::Position.to_string()));
    }

    #[test]
    fn convert_cover_entity_with_partial_features() {
        let entity_id = "cover.partial".to_string();
        let state = "closed".to_string();
        let supported_features = COVER_SUPPORT_OPEN | COVER_SUPPORT_CLOSE;

        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Partial Cover",
            "supported_features": supported_features
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert_eq!(2, features.len());
        assert!(features.contains(&CoverFeature::Open.to_string()));
        assert!(features.contains(&CoverFeature::Close.to_string()));
        assert!(!features.contains(&CoverFeature::Stop.to_string()));
        assert!(!features.contains(&CoverFeature::Position.to_string()));
    }

    #[test]
    fn convert_cover_entity_missing_supported_features() {
        let entity_id = "cover.no_features".to_string();
        let state = "closed".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "No Features Cover"
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Should have no features when supported_features is missing
        assert_eq!(0, features.len());
    }

    #[test]
    fn convert_cover_entity_invalid_supported_features() {
        let entity_id = "cover.invalid_features".to_string();
        let state = "open".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Invalid Features Cover",
            "supported_features": "not_a_number"
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Should default to 0 and have no features
        assert_eq!(0, features.len());
    }

    #[test]
    fn convert_cover_entity_structure() {
        let entity_id = "cover.structure_test".to_string();
        let state = "open".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Structure Test Cover"
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id.clone(), state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();

        // Verify required fields
        assert_eq!(entity_id, entity.entity_id);
        assert_eq!(None, entity.device_id);
        assert_eq!(EntityType::Cover, entity.entity_type);
        assert_eq!(None, entity.area);
        assert_eq!(None, entity.options);
        assert!(entity.name.contains_key("en"));
        assert!(entity.features.is_some());
        assert!(entity.attributes.is_some());
    }

    #[test]
    fn convert_cover_entity_with_position_attributes() {
        let entity_id = "cover.position_cover".to_string();
        let state = "open".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Position Cover",
            "supported_features": COVER_SUPPORT_SET_POSITION,
            "current_position": 75
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert!(entity.attributes.is_some());

        let attributes = entity.attributes.unwrap();
        assert_eq!(Some(&json!("OPEN")), attributes.get("state"));
        assert_eq!(Some(&json!(75)), attributes.get("position"));
    }

    #[test]
    fn convert_cover_entity_with_tilt_position_attributes() {
        let entity_id = "cover.tilt_cover".to_string();
        let state = "open".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Tilt Cover",
            "current_tilt_position": 45
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert!(entity.attributes.is_some());

        let attributes = entity.attributes.unwrap();
        assert_eq!(Some(&json!("OPEN")), attributes.get("state"));
        assert_eq!(Some(&json!(45)), attributes.get("tilt_position"));
    }

    #[test]
    fn convert_cover_entity_with_invalid_position_attributes() {
        let entity_id = "cover.invalid_position".to_string();
        let state = "open".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Invalid Position Cover",
            "current_position": 150,  // Invalid: > 100
            "current_tilt_position": -10  // Invalid: < 0
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert!(entity.attributes.is_some());

        let attributes = entity.attributes.unwrap();
        assert_eq!(Some(&json!("OPEN")), attributes.get("state"));
        // Invalid values should be ignored
        assert!(attributes.get("position").is_none());
        assert!(attributes.get("tilt_position").is_none());
    }

    #[test]
    fn convert_cover_entity_with_both_position_attributes() {
        let entity_id = "cover.both_positions".to_string();
        let state = "open".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Both Positions Cover",
            "current_position": 80,
            "current_tilt_position": 60
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert!(entity.attributes.is_some());

        let attributes = entity.attributes.unwrap();
        assert_eq!(Some(&json!("OPEN")), attributes.get("state"));
        assert_eq!(Some(&json!(80)), attributes.get("position"));
        assert_eq!(Some(&json!(60)), attributes.get("tilt_position"));
    }

    #[test]
    fn convert_cover_entity_with_device_class_and_features() {
        let entity_id = "cover.garage_door".to_string();
        let state = "closed".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Garage Door",
            "device_class": "garage",
            "supported_features": COVER_SUPPORT_OPEN | COVER_SUPPORT_CLOSE | COVER_SUPPORT_STOP
        }))
        .unwrap();

        let result = convert_cover_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();

        assert_eq!(Some("garage".to_string()), entity.device_class);
        let features = entity.features.unwrap();
        assert_eq!(3, features.len());
        assert!(features.contains(&CoverFeature::Open.to_string()));
        assert!(features.contains(&CoverFeature::Close.to_string()));
        assert!(features.contains(&CoverFeature::Stop.to_string()));
    }
}
