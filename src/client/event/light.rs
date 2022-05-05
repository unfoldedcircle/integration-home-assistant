// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Light entity specific HA event logic.

use log::info;
use serde_json::Value;

use uc_api::{intg::EntityChange, EntityType};

use crate::client::event::convert_ha_onoff_state;
use crate::client::model::EventData;
use crate::errors::ServiceError;

pub(crate) fn light_event_to_entity_change(data: EventData) -> Result<EntityChange, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(2);
    let state = convert_ha_onoff_state(&data.new_state.state)?;

    attributes.insert("state".to_string(), state);

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
                    let mireds = max_mireds - min_mireds;
                    if mireds > 0 {
                        // TODO
                        info!("TODO implement mired color temperature conversion for value: {} [{}..{}]", color_temp, min_mireds, max_mireds );
                    }
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
                    if hue > 255 || saturation > 360 {
                        return Err(ServiceError::BadRequest(format!(
                            "Invalid hs_color values ({}, {})",
                            hue, saturation
                        )));
                    }
                    attributes.insert("hue".to_string(), Value::Number(hue.into()));
                    attributes.insert("saturation".to_string(), Value::Number(saturation.into()));
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
