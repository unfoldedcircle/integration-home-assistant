// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Sensor entity specific logic.

use crate::client::event::convert_ha_onoff_state;
use crate::client::model::EventData;
use crate::errors::ServiceError;
use serde_json::{Map, Value};
use std::collections::HashMap;
use uc_api::intg::AvailableIntgEntity;
use uc_api::{EntityType, SensorOptionField, intg::EntityChange};

pub(crate) fn map_sensor_attributes(
    _entity_id: &str,
    state: &str,
    ha_attr: Option<&mut Map<String, Value>>,
) -> Result<Map<String, Value>, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(2);
    attributes.insert("value".into(), state.into());

    if let Some(ha_attr) = ha_attr
        && let Some(uom) = ha_attr.remove("unit_of_measurement")
    {
        attributes.insert("unit".into(), uom);
    }
    // TODO check and handle attributes.device_class? E.g. checking for supported sensors.
    // Currently supported: "battery" | "current" | "energy" | "humidity" | "power" | "temperature" | "voltage"

    Ok(attributes)
}

pub(crate) fn sensor_event_to_entity_change(
    mut data: EventData,
) -> Result<EntityChange, ServiceError> {
    let attributes = map_sensor_attributes(
        &data.entity_id,
        &data.new_state.state,
        data.new_state.attributes.as_mut(),
    )?;

    Ok(EntityChange {
        device_id: None, // prepared for device_id handling
        entity_type: EntityType::Sensor,
        entity_id: data.entity_id,
        attributes,
    })
}

pub(crate) fn binary_sensor_event_to_entity_change(
    data: EventData,
) -> Result<EntityChange, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(3);
    let state = convert_ha_onoff_state(&data.new_state.state)?;

    // TODO decide on how to handle the special binary sensor #13
    attributes.insert("value".into(), (Some("ON") == state.as_str()).into());
    attributes.insert("state".into(), state);
    attributes.insert("unit".into(), "boolean".into());

    Ok(EntityChange {
        device_id: None,
        entity_type: EntityType::Sensor,
        entity_id: data.entity_id,
        attributes,
    })
}

pub(crate) fn convert_sensor_entity(
    entity_id: String,
    state: String,
    ha_attr: &mut Map<String, Value>,
) -> Result<AvailableIntgEntity, ServiceError> {
    let friendly_name = ha_attr.get("friendly_name").and_then(|v| v.as_str());
    let name = HashMap::from([("en".into(), friendly_name.unwrap_or(&entity_id).into())]);
    let mut options = serde_json::Map::new();
    let device_class = ha_attr.get("device_class").and_then(|v| v.as_str());
    let device_class = match device_class {
        Some("battery") | Some("current") | Some("energy") | Some("humidity") | Some("power")
        | Some("temperature") | Some("voltage") => device_class.map(|v| v.into()),
        // Map non-supported device classes to a custom sensor and use device class as label
        v => {
            if let Some(v) = v
                && let Some(label) = device_class_to_label(v)
            {
                options.insert(
                    SensorOptionField::CustomLabel.to_string(),
                    Value::String(label),
                );
            }
            if let Some(v) = ha_attr.get("unit_of_measurement") {
                options.insert(SensorOptionField::CustomUnit.to_string(), v.clone());
            }
            Some("custom".into())
        }
    };

    // convert attributes
    let attributes = Some(map_sensor_attributes(&entity_id, &state, Some(ha_attr))?);

    Ok(AvailableIntgEntity {
        entity_id,
        device_id: None, // prepared for device_id handling
        entity_type: EntityType::Sensor,
        device_class,
        name,
        features: None,
        area: None,
        options: None,
        attributes,
    })
}

fn device_class_to_label(class: &str) -> Option<String> {
    let name = class.replace('_', " ");
    let mut c = name.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
}
