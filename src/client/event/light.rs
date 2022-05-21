// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Light entity specific HA event logic.

use log::{info, warn};
use serde_json::Value;

use uc_api::{intg::EntityChange, EntityType};

use crate::client::event::convert_ha_onoff_state;
use crate::client::model::EventData;
use crate::errors::ServiceError;

pub(crate) fn light_event_to_entity_change(data: EventData) -> Result<EntityChange, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(2);
    let state = convert_ha_onoff_state(&data.new_state.state)?;

    attributes.insert("state".into(), state);

    if let Some(mut ha_attr) = data.new_state.attributes {
        // FIXME brightness adjustment for RGB## modes. https://developers.home-assistant.io/docs/core/entity/light
        // Note that in color modes COLOR_MODE_RGB, COLOR_MODE_RGBW and COLOR_MODE_RGBWW there is
        // brightness information both in the light's brightness property and in the color. As an
        // example, if the light's brightness is 128 and the light's color is (192, 64, 32), the
        // overall brightness of the light is: 128/255 * max(192, 64, 32)/255 = 38%.
        ha_attr
            .remove_entry("brightness")
            .and_then(|e| match e.1.is_u64() {
                true => Some(e),
                false => None,
            })
            .map(|e| attributes.insert(e.0, e.1));

        match ha_attr.get("color_mode").and_then(|v| v.as_str()) {
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
                if let Some(hs) = ha_attr.get("hs_color").and_then(|v| v.as_array()) {
                    if hs.len() != 2 {
                        return Err(ServiceError::BadRequest(
                            "Invalid hs_color value. Expected hue & saturation".into(),
                        ));
                    }
                    // hs values are returned as floats: hue: 0..360, saturation: 0..100
                    let hue = hs.get(0).unwrap().as_f64().unwrap_or_default() as u16;
                    let saturation =
                        (hs.get(1).unwrap().as_f64().unwrap_or_default() as f32 * 2.55_f32) as u16;
                    if hue > 360 || saturation > 100 {
                        return Err(ServiceError::BadRequest(format!(
                            "Invalid hs_color values ({}, {})",
                            hue, saturation
                        )));
                    }
                    attributes.insert("hue".into(), Value::Number(hue.into()));
                    attributes.insert(
                        "saturation".into(),
                        Value::Number((saturation * 255 / 100).into()),
                    );
                }
            }
            None => {}
            v => {
                info!(
                    "TODO implement color mode conversion for color_mode: {}",
                    v.unwrap()
                );
            }
        }
    }

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

#[cfg(test)]
mod tests {
    use crate::client::event::light::color_temp_mired_to_percent;
    use crate::errors::ServiceError;
    use rstest::rstest;

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
}
