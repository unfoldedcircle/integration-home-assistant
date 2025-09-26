// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Light entity specific logic.

use crate::client::event::convert_ha_onoff_state;
use crate::client::model::EventData;
use crate::errors::ServiceError;
use crate::util::{color_rgb_to_hsv, color_xy_to_hs};
use log::warn;
use serde_json::{Map, Value};
use std::collections::HashMap;
use uc_api::intg::AvailableIntgEntity;
use uc_api::{EntityType, LightFeature, intg::EntityChange};

pub(crate) fn map_light_attributes(
    entity_id: &str,
    state: &str,
    ha_attr: Option<&mut Map<String, Value>>,
) -> Result<Map<String, Value>, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(2);
    let state = convert_ha_onoff_state(state)?;

    attributes.insert("state".into(), state);

    if let Some(ha_attr) = ha_attr {
        ha_attr
            .remove_entry("brightness")
            .and_then(|(key, value)| match value.is_u64() {
                true => Some((key, value)),
                false => None,
            })
            .map(|(key, value)| attributes.insert(key, value));

        // Color modes in HA are quite confusing...
        match ha_attr.get("color_mode").and_then(|v| v.as_str()) {
            Some("brightness") => {
                // simply ignore, we already got the brightness value
            }
            Some("color_temp") => {
                if let Some(color_temp) = ha_attr.get("color_temp").and_then(|v| v.as_u64()) {
                    let min_mireds = ha_attr
                        .get("min_mireds")
                        .and_then(|v| v.as_u64())
                        .unwrap_or_default() as u16;
                    let max_mireds = ha_attr
                        .get("max_mireds")
                        .and_then(|v| v.as_u64())
                        .unwrap_or_default() as u16;

                    let color_temp_pct =
                        color_temp_mired_to_percent(color_temp, min_mireds, max_mireds)?;

                    attributes.insert(
                        "color_temperature".into(),
                        Value::Number(color_temp_pct.into()),
                    );
                }
            }
            Some("hs") => {
                // Easiest one since R2 uses HS as well
                extract_hs_color(ha_attr, &mut attributes)?;
            }
            Some("xy") => {
                // First check if HA is so kind to provide hs values.
                // It seems that all color models are provided, but couldn't find documentation
                if !extract_hs_color(ha_attr, &mut attributes)? {
                    // Nope, use xy and convert
                    extract_xy_color(ha_attr, &mut attributes)?;
                }
            }
            Some("rgb") | Some("rgbw") | Some("rgbww") => {
                // Same procedure as for xy and the assumption that HA provides converted color model
                if !extract_hs_color(ha_attr, &mut attributes)? {
                    extract_rgb_color(ha_attr, &mut attributes)?;
                }
            }
            // Some("white") => {} // TODO #7 check if we need to handle white color model
            Some("onoff") => {
                // nothing to do, HA docs: The light can be turned on or off. This mode must be the only supported mode if supported by the light.
            }
            Some("unknown") => {}
            None => {}
            v => {
                warn!(
                    "Unhandled color_mode '{}' in entity {entity_id}, ha_attr: {ha_attr:?}",
                    v.unwrap()
                );
            }
        }
    }

    Ok(attributes)
}

pub(crate) fn light_event_to_entity_change(
    mut data: EventData,
) -> Result<EntityChange, ServiceError> {
    let attributes = map_light_attributes(
        &data.entity_id,
        &data.new_state.state,
        data.new_state.attributes.as_mut(),
    )?;

    Ok(EntityChange {
        device_id: None,
        entity_type: EntityType::Light,
        entity_id: data.entity_id,
        attributes,
    })
}

fn color_temp_mired_to_percent(
    mut value: u64,
    min_mireds: u16,
    max_mireds: u16,
) -> Result<u16, ServiceError> {
    if max_mireds <= min_mireds {
        return Err(ServiceError::BadRequest(format!(
            "Invalid min_mireds or max_mireds value! min_mireds={}, max_mireds={}",
            min_mireds, max_mireds
        )));
    }
    if (value as u16) < min_mireds {
        warn!(
            "Adjusted invalid color_temp value {} to: {}",
            value, min_mireds
        );
        value = min_mireds as u64;
    }
    if (value as u16) > max_mireds {
        warn!(
            "Adjusted invalid color_temp value {} to: {}",
            value, max_mireds
        );
        value = max_mireds as u64;
    }

    Ok(((value as u16) - min_mireds) * 100 / (max_mireds - min_mireds))
}

pub(crate) fn convert_light_entity(
    entity_id: String,
    state: String,
    ha_attr: &mut Map<String, Value>,
) -> Result<AvailableIntgEntity, ServiceError> {
    let friendly_name = ha_attr.get("friendly_name").and_then(|v| v.as_str());
    let name = HashMap::from([("en".into(), friendly_name.unwrap_or(&entity_id).into())]);

    // handle features
    let mut light_feats = Vec::with_capacity(2);
    // OnOff is default
    light_feats.push(LightFeature::Toggle);

    if let Some(color_modes) = ha_attr
        .get("supported_color_modes")
        .and_then(|v| v.as_array())
    {
        let mut dim = false;
        let mut color = false;
        let mut color_temp = false;
        for color_mode in color_modes {
            match color_mode.as_str().unwrap_or_default() {
                "brightness" => dim = true,
                "color_temp" => {
                    dim = true;
                    color_temp = true;
                }
                "hs" | "rgb" | "rgbw" | "rgbww" | "xy" => {
                    dim = true;
                    color = true;
                }
                &_ => continue,
            };
        }
        if dim {
            light_feats.push(LightFeature::Dim);
        }
        if color {
            light_feats.push(LightFeature::Color);
        }
        if color_temp {
            light_feats.push(LightFeature::ColorTemperature);
        }
    }

    // TODO color entity options: color_temperature_steps - do we get that from HASS? #8

    // convert attributes
    let attributes = Some(map_light_attributes(&entity_id, &state, Some(ha_attr))?);

    Ok(AvailableIntgEntity {
        entity_id,
        device_id: None, // prepared for device_id handling
        entity_type: EntityType::Light,
        device_class: None,
        name,
        features: Some(light_feats.into_iter().map(|v| v.to_string()).collect()),
        area: None,
        options: None,
        attributes,
    })
}

/// Extract and convert `hs_color` field from the HA attributes.
///
/// Expects an array of two float values containing hue and saturation values.
/// - Hue range: 0..360
/// - Saturation range: 0..100
///
/// # Arguments
///
/// * `ha_attr`: Input Home Assistant light entity attributes
/// * `attributes`: Output R2 light entity attributes
///
/// returns:
/// - true if input value is present and was converted into the output attributes
/// - false if the input value is not present
/// - ServiceError::BadRequest if the input value is in an invalid format
fn extract_hs_color(
    ha_attr: &Map<String, Value>,
    attributes: &mut Map<String, Value>,
) -> Result<bool, ServiceError> {
    if let Some(hs) = ha_attr.get("hs_color").and_then(|v| v.as_array()) {
        if hs.len() != 2 {
            return Err(ServiceError::BadRequest(
                "Invalid hs_color value. Expected array with hue & saturation".into(),
            ));
        }
        // hs values are returned as floats: hue: 0..360, saturation: 0..100
        let hue = hs.first().unwrap().as_f64().unwrap_or_default().round() as u16;
        let saturation = hs.get(1).unwrap().as_f64().unwrap_or_default().round() as f32;
        if hue > 360 || saturation > 100.0 {
            return Err(ServiceError::BadRequest(format!(
                "Invalid hs_color values ({}, {})",
                hue, saturation
            )));
        }
        attributes.insert("hue".into(), Value::Number(hue.into()));
        attributes.insert(
            "saturation".into(),
            Value::Number(((saturation * 255. / 100.).round() as u16).into()),
        );
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Extract and convert `xy_color` field from the HA attributes.
///
/// Expects an array of two float values containing x and y values (0..1).
///
/// # Arguments
///
/// * `ha_attr`: Input Home Assistant light entity attributes
/// * `attributes`: Output R2 light entity attributes
///
/// returns:
/// - true if input value is present and was converted into the output attributes
/// - false if the input value is not present
/// - ServiceError::BadRequest if the input value is in an invalid format
fn extract_xy_color(
    ha_attr: &Map<String, Value>,
    attributes: &mut Map<String, Value>,
) -> Result<bool, ServiceError> {
    if let Some(xy) = ha_attr.get("xy_color").and_then(|v| v.as_array()) {
        if xy.len() != 2 {
            return Err(ServiceError::BadRequest(
                "Invalid xy_color value. Expected array with x & y".into(),
            ));
        }
        // xy values are returned as floats: 0..1
        let x = xy.first().unwrap().as_f64().unwrap_or_default() as f32;
        let y = xy.get(1).unwrap().as_f64().unwrap_or_default() as f32;

        let (hue, saturation) = color_xy_to_hs(x, y, None);
        attributes.insert("hue".into(), Value::Number((hue.round() as u16).into()));
        attributes.insert(
            "saturation".into(),
            Value::Number(((saturation * 255. / 100.).round() as u16).into()),
        );
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Extract and convert `rgb_color` field from the HA attributes.
///
/// Expects an array of three integer values containing r, g, b values (0..255).
///
/// # Arguments
///
/// * `ha_attr`: Input Home Assistant light entity attributes
/// * `attributes`: Output R2 light entity attributes
///
/// returns:
/// - true if input value is present and was converted into the output attributes
/// - false if the input value is not present
/// - ServiceError::BadRequest if the input value is in an invalid format
fn extract_rgb_color(
    ha_attr: &Map<String, Value>,
    attributes: &mut Map<String, Value>,
) -> Result<bool, ServiceError> {
    if let Some(rgb) = ha_attr.get("rgb_color").and_then(|v| v.as_array()) {
        if rgb.len() != 3 {
            return Err(ServiceError::BadRequest(
                "Invalid rgb_color value. Expected array with r,g,b".into(),
            ));
        }
        // rgb values are returned as u16: 0..255
        let r = rgb.first().unwrap().as_u64().unwrap_or_default() as f32;
        let g = rgb.get(1).unwrap().as_u64().unwrap_or_default() as f32;
        let b = rgb.get(2).unwrap().as_u64().unwrap_or_default() as f32;

        let (hue, saturation, _) = color_rgb_to_hsv(r, g, b);
        attributes.insert("hue".into(), Value::Number((hue.round() as u16).into()));
        attributes.insert(
            "saturation".into(),
            Value::Number(((saturation * 255. / 100.).round() as u16).into()),
        );
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::ServiceError;
    use rstest::rstest;
    use serde_json::json;
    use uc_api::{EntityType, LightFeature};

    #[rstest]
    #[case(0, 0)]
    #[case(50, 0)]
    #[case(149, 0)]
    #[case(501, 100)]
    #[case(1000, 100)]
    fn color_temp_mired_to_percent_with_invalid_input_adjusts_value(
        #[case] input: u64,
        #[case] expected: u16,
    ) {
        let result = color_temp_mired_to_percent(input, 150, 500);
        assert_eq!(Ok(expected), result);
    }

    #[rstest]
    #[case(150, 150)]
    #[case(200, 150)]
    fn color_temp_mired_to_percent_with_invalid_min_max_mireds_returns_err(
        #[case] min_mireds: u16,
        #[case] max_mireds: u16,
    ) {
        let result = color_temp_mired_to_percent(150, min_mireds, max_mireds);
        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "Invalid min_ / max_mireds value must return BadRequest"
        );
    }

    #[rstest]
    #[case(0, 150)]
    #[case(1, 154)]
    #[case(50, 325)]
    #[case(99, 497)]
    #[case(100, 500)]
    fn color_temp_mired_to_percent_returns_scaled_values(
        #[case] expected: u16,
        #[case] input: u64,
    ) {
        let min_mireds = 150;
        let max_mireds = 500;
        let result = color_temp_mired_to_percent(input, min_mireds, max_mireds);

        assert_eq!(Ok(expected), result);
    }

    #[test]
    fn convert_light_entity_basic() {
        let entity_id = "light.living_room".to_string();
        let state = "on".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Living Room Light"
        }))
        .unwrap();

        let result = convert_light_entity(entity_id.clone(), state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(entity_id, entity.entity_id);
        assert_eq!(EntityType::Light, entity.entity_type);
        assert_eq!(None, entity.device_class);
        assert_eq!(
            Some(&"Living Room Light".to_string()),
            entity.name.get("en")
        );
        assert!(entity.features.is_some());
        assert!(entity.attributes.is_some());

        // Should have at least the toggle feature
        let features = entity.features.unwrap();
        assert!(features.contains(&LightFeature::Toggle.to_string()));
    }

    #[test]
    fn convert_light_entity_no_friendly_name() {
        let entity_id = "light.bedroom".to_string();
        let state = "off".to_string();
        let mut ha_attr = serde_json::from_value(json!({})).unwrap();

        let result = convert_light_entity(entity_id.clone(), state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(Some(&entity_id), entity.name.get("en"));
    }

    #[rstest]
    #[case("on")]
    #[case("off")]
    #[case("unavailable")]
    #[case("unknown")]
    fn convert_light_entity_all_states(#[case] state: &str) {
        let entity_id = format!("light.state_test_{}", state);
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": format!("State Test {}", state)
        }))
        .unwrap();

        let result = convert_light_entity(entity_id, state.to_string(), &mut ha_attr);

        assert!(result.is_ok(), "Failed for state: {}", state);
        let entity = result.unwrap();
        assert!(entity.attributes.is_some());
    }

    #[test]
    fn convert_light_entity_with_brightness_only() {
        let entity_id = "light.dimmable".to_string();
        let state = "on".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Dimmable Light",
            "supported_color_modes": ["brightness"]
        }))
        .unwrap();

        let result = convert_light_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(features.contains(&LightFeature::Toggle.to_string()));
        assert!(features.contains(&LightFeature::Dim.to_string()));
        assert!(!features.contains(&LightFeature::Color.to_string()));
        assert!(!features.contains(&LightFeature::ColorTemperature.to_string()));
    }

    #[test]
    fn convert_light_entity_with_color_temperature() {
        let entity_id = "light.temp_light".to_string();
        let state = "on".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Temperature Light",
            "supported_color_modes": ["color_temp"]
        }))
        .unwrap();

        let result = convert_light_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(features.contains(&LightFeature::Toggle.to_string()));
        assert!(features.contains(&LightFeature::Dim.to_string()));
        assert!(features.contains(&LightFeature::ColorTemperature.to_string()));
        assert!(!features.contains(&LightFeature::Color.to_string()));
    }

    #[rstest]
    #[case("hs")]
    #[case("rgb")]
    #[case("rgbw")]
    #[case("rgbww")]
    #[case("xy")]
    fn convert_light_entity_with_color_modes(#[case] color_mode: &str) {
        let entity_id = format!("light.color_{}", color_mode);
        let state = "on".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": format!("Color Light {}", color_mode),
            "supported_color_modes": [color_mode]
        }))
        .unwrap();

        let result = convert_light_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Failed for color mode: {}", color_mode);
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(features.contains(&LightFeature::Toggle.to_string()));
        assert!(features.contains(&LightFeature::Dim.to_string()));
        assert!(features.contains(&LightFeature::Color.to_string()));
        assert!(!features.contains(&LightFeature::ColorTemperature.to_string()));
    }

    #[test]
    fn convert_light_entity_with_all_color_modes() {
        let entity_id = "light.full_featured".to_string();
        let state = "on".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Full Featured Light",
            "supported_color_modes": ["brightness", "color_temp", "hs", "rgb", "xy"]
        }))
        .unwrap();

        let result = convert_light_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(features.contains(&LightFeature::Toggle.to_string()));
        assert!(features.contains(&LightFeature::Dim.to_string()));
        assert!(features.contains(&LightFeature::Color.to_string()));
        assert!(features.contains(&LightFeature::ColorTemperature.to_string()));
    }

    #[test]
    fn convert_light_entity_no_color_modes() {
        let entity_id = "light.simple".to_string();
        let state = "on".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Simple Light"
        }))
        .unwrap();

        let result = convert_light_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Should only have toggle feature
        assert_eq!(1, features.len());
        assert!(features.contains(&LightFeature::Toggle.to_string()));
    }

    #[test]
    fn convert_light_entity_empty_color_modes() {
        let entity_id = "light.empty_modes".to_string();
        let state = "on".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Empty Modes Light",
            "supported_color_modes": []
        }))
        .unwrap();

        let result = convert_light_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Should only have toggle feature
        assert_eq!(1, features.len());
        assert!(features.contains(&LightFeature::Toggle.to_string()));
    }

    #[test]
    fn convert_light_entity_invalid_color_modes() {
        let entity_id = "light.invalid_modes".to_string();
        let state = "on".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Invalid Modes Light",
            "supported_color_modes": ["unknown_mode", "invalid"]
        }))
        .unwrap();

        let result = convert_light_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        // Should only have toggle feature since no valid modes
        assert_eq!(1, features.len());
        assert!(features.contains(&LightFeature::Toggle.to_string()));
    }

    #[test]
    fn convert_light_entity_structure() {
        let entity_id = "light.structure_test".to_string();
        let state = "on".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Structure Test Light"
        }))
        .unwrap();

        let result = convert_light_entity(entity_id.clone(), state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();

        // Verify required fields
        assert_eq!(entity_id, entity.entity_id);
        assert_eq!(None, entity.device_id);
        assert_eq!(EntityType::Light, entity.entity_type);
        assert_eq!(None, entity.device_class);
        assert_eq!(None, entity.area);
        assert_eq!(None, entity.options);
        assert!(entity.name.contains_key("en"));
        assert!(entity.features.is_some());
        assert!(entity.attributes.is_some());
    }

    #[test]
    fn convert_light_entity_with_brightness_attribute() {
        let entity_id = "light.bright_light".to_string();
        let state = "on".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Bright Light",
            "supported_color_modes": ["brightness"],
            "brightness": 128
        }))
        .unwrap();

        let result = convert_light_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert!(entity.attributes.is_some());

        let attributes = entity.attributes.unwrap();
        assert_eq!(Some(&json!("ON")), attributes.get("state"));
        assert_eq!(Some(&json!(128)), attributes.get("brightness"));
    }

    #[test]
    fn convert_light_entity_with_color_attributes() {
        let entity_id = "light.color_light".to_string();
        let state = "on".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Color Light",
            "supported_color_modes": ["hs"],
            "color_mode": "hs",
            "hs_color": [120.0, 75.0]
        }))
        .unwrap();

        let result = convert_light_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert!(entity.attributes.is_some());

        let attributes = entity.attributes.unwrap();
        assert_eq!(Some(&json!("ON")), attributes.get("state"));
        assert_eq!(Some(&json!(120)), attributes.get("hue"));
        // saturation converted from 0-100 to 0-255 range
        assert_eq!(Some(&json!(191)), attributes.get("saturation"));
    }

    #[test]
    fn convert_light_entity_with_color_temp_attributes() {
        let entity_id = "light.temp_light".to_string();
        let state = "on".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Temperature Light",
            "supported_color_modes": ["color_temp"],
            "color_mode": "color_temp",
            "color_temp": 300,
            "min_mireds": 150,
            "max_mireds": 500
        }))
        .unwrap();

        let result = convert_light_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert!(entity.attributes.is_some());

        let attributes = entity.attributes.unwrap();
        assert_eq!(Some(&json!("ON")), attributes.get("state"));
        assert_eq!(Some(&json!(42)), attributes.get("color_temperature"));
    }

    #[test]
    fn convert_light_entity_mixed_brightness_and_color() {
        let entity_id = "light.mixed".to_string();
        let state = "on".to_string();
        let mut ha_attr = serde_json::from_value(json!({
            "friendly_name": "Mixed Light",
            "supported_color_modes": ["brightness", "hs", "color_temp"]
        }))
        .unwrap();

        let result = convert_light_entity(entity_id, state, &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        let features = entity.features.unwrap();

        assert!(features.contains(&LightFeature::Toggle.to_string()));
        assert!(features.contains(&LightFeature::Dim.to_string()));
        assert!(features.contains(&LightFeature::Color.to_string()));
        assert!(features.contains(&LightFeature::ColorTemperature.to_string()));
        assert_eq!(4, features.len());
    }
}
