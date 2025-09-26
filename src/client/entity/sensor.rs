// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Sensor entity-specific logic.

use crate::client::event::convert_ha_sensor_state;
use crate::client::model::EventData;
use crate::errors::ServiceError;
use serde_json::{Map, Value};
use std::collections::HashMap;
use uc_api::intg::AvailableIntgEntity;
use uc_api::{
    EntityType, SensorAttribute, SensorDeviceClass, SensorOptionField, intg::EntityChange,
};

fn map_sensor_attributes(
    entity_id: &str,
    state: &str,
    ha_attr: Option<&mut Map<String, Value>>,
) -> Result<Map<String, Value>, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(2);
    attributes.insert(
        SensorAttribute::State.to_string(),
        convert_ha_sensor_state(state)?,
    );
    // HA: the state of a sensor entity is its currently detected value, which can be either text or a number.
    //     In addition, the entity can have the following states: unavailable, unknown
    //     https://www.home-assistant.io/integrations/sensor
    attributes.insert(SensorAttribute::Value.to_string(), state.into());

    // map HA binary-sensor device class into UC sensor unit field
    if entity_id.starts_with("binary_sensor.") {
        if let Some(ha_attr) = ha_attr
            && let Some(class) = ha_attr
                .remove("device_class")
                .and_then(|v| v.as_str().map(|v| v.to_lowercase()))
            && class != "none"
        {
            attributes.insert(SensorAttribute::Unit.to_string(), class.into());
        }
        return Ok(attributes);
    }

    if let Some(ha_attr) = ha_attr
        && let Some(uom) = ha_attr.remove("unit_of_measurement")
    {
        attributes.insert(SensorAttribute::Unit.to_string(), uom);
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
        _ if entity_id.starts_with("binary_sensor.") => Some(SensorDeviceClass::Binary.to_string()),
        // supported device classes
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
            Some(SensorDeviceClass::Custom.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uc_api::{SensorAttribute, SensorDeviceClass};

    #[test]
    fn convert_sensor_entity_mappings() {
        let entity_id = "sensor.temperature".to_string();
        let state = "23.5".to_string();
        let mut ha_attr = serde_json::Map::new();
        ha_attr.insert(
            "friendly_name".to_string(),
            json!("Living Room Temperature"),
        );
        ha_attr.insert("device_class".to_string(), json!("temperature"));
        ha_attr.insert("unit_of_measurement".to_string(), json!("°C"));

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(entity_id, entity.entity_id);
        assert_eq!(EntityType::Sensor, entity.entity_type);
        assert_eq!(Some("temperature".to_string()), entity.device_class);
        assert_eq!("Living Room Temperature", entity.name.get("en").unwrap());

        let attributes = entity.attributes.unwrap();
        assert_eq!(
            Some(&json!("ON")),
            attributes.get(SensorAttribute::State.as_ref())
        );
        assert_eq!(
            Some(&json!("23.5")),
            attributes.get(SensorAttribute::Value.as_ref())
        );
        assert_eq!(
            Some(&json!("°C")),
            attributes.get(SensorAttribute::Unit.as_ref())
        );
    }

    #[test]
    fn convert_binary_sensor_entity() {
        let entity_id = "binary_sensor.door_sensor".to_string();
        let state = "on".to_string();
        let mut ha_attr = serde_json::Map::new();
        ha_attr.insert("friendly_name".to_string(), json!("Front Door"));
        ha_attr.insert("device_class".to_string(), json!("door"));

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(entity_id, entity.entity_id);
        assert_eq!(
            Some(SensorDeviceClass::Binary.to_string()),
            entity.device_class
        );

        let attributes = entity.attributes.unwrap();
        assert_eq!(
            Some(&json!("ON")),
            attributes.get(SensorAttribute::State.as_ref())
        );
        assert_eq!(
            Some(&json!("on")),
            attributes.get(SensorAttribute::Value.as_ref())
        );
        assert_eq!(
            Some(&json!("door")),
            attributes.get(SensorAttribute::Unit.as_ref())
        );
    }

    #[test]
    fn convert_sensor_entity_with_supported_device_classes() {
        let supported_classes = vec![
            "battery",
            "current",
            "energy",
            "humidity",
            "power",
            "temperature",
            "voltage",
        ];

        for device_class in supported_classes {
            let entity_id = format!("sensor.test_{}", device_class);
            let state = "100".to_string();
            let mut ha_attr = serde_json::Map::new();
            ha_attr.insert("device_class".to_string(), json!(device_class));
            ha_attr.insert("unit_of_measurement".to_string(), json!("unit"));

            let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr);

            assert!(result.is_ok(), "Failed for device class: {}", device_class);
            let entity = result.unwrap();
            assert_eq!(Some(device_class.to_string()), entity.device_class);
        }
    }

    #[test]
    fn convert_sensor_entity_without_friendly_name_maps_name_to_entity_id() {
        let entity_id = "sensor.unnamed".to_string();
        let state = "42".to_string();
        let mut ha_attr = serde_json::Map::new();
        ha_attr.insert("device_class".to_string(), json!("power"));

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(
            Some(SensorDeviceClass::Power.to_string()),
            entity.device_class
        );
        assert_eq!(&entity_id, entity.name.get("en").unwrap());
    }

    #[test]
    fn convert_sensor_entity_without_device_class_maps_to_custom() {
        let entity_id = "sensor.generic".to_string();
        let state = "unknown".to_string();
        let unit = "units";
        let mut ha_attr = serde_json::Map::new();
        ha_attr.insert("friendly_name".to_string(), json!("Generic Sensor"));
        ha_attr.insert("unit_of_measurement".to_string(), json!(unit));

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(
            Some(SensorDeviceClass::Custom.to_string()),
            entity.device_class
        );

        let attributes = entity.attributes.unwrap();
        assert_eq!(
            Some(&json!(unit)),
            attributes.get(SensorAttribute::Unit.as_ref())
        );
    }

    #[test]
    fn convert_sensor_entity_unavailable_state() {
        let entity_id = "sensor.offline".to_string();
        let state = "unavailable".to_string();
        let mut ha_attr = serde_json::Map::new();
        ha_attr.insert("friendly_name".to_string(), json!("Offline Sensor"));
        ha_attr.insert("device_class".to_string(), json!("temperature"));

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(
            Some(SensorDeviceClass::Temperature.to_string()),
            entity.device_class
        );

        let attributes = entity.attributes.unwrap();
        assert_eq!(
            Some(&json!("UNAVAILABLE")),
            attributes.get(SensorAttribute::State.as_ref())
        );
        assert_eq!(
            Some(&json!(state)),
            attributes.get(SensorAttribute::Value.as_ref())
        );
    }

    #[test]
    fn convert_sensor_entity_with_unknown_state() {
        let entity_id = "sensor.mystery".to_string();
        let state = "unknown".to_string();
        let mut ha_attr = serde_json::Map::new();
        ha_attr.insert("friendly_name".to_string(), json!("Mystery Sensor"));
        ha_attr.insert("device_class".to_string(), json!("humidity"));

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();

        let attributes = entity.attributes.unwrap();
        assert_eq!(
            Some(&json!("UNKNOWN")),
            attributes.get(SensorAttribute::State.as_ref())
        );
        assert_eq!(
            Some(&json!(state)),
            attributes.get(SensorAttribute::Value.as_ref())
        );
    }

    #[test]
    fn convert_sensor_entity_binary_sensor_with_invalid_state_maps_to_on_state() {
        let entity_id = "binary_sensor.generic".to_string();
        let state = "off".to_string(); // sensors have no off state
        let mut ha_attr = serde_json::Map::new();
        ha_attr.insert("friendly_name".to_string(), json!("Generic Binary"));

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(
            Some(SensorDeviceClass::Binary.to_string()),
            entity.device_class
        );

        let attributes = entity.attributes.unwrap();
        assert_eq!(
            Some(&json!("ON")),
            attributes.get(SensorAttribute::State.as_ref())
        );
        assert_eq!(
            Some(&json!(state)),
            attributes.get(SensorAttribute::Value.as_ref())
        );
        // No unit should be set when device_class is missing
        assert!(
            attributes.get(SensorAttribute::Unit.as_ref()).is_none(),
            "No unit expected"
        );
    }

    #[test]
    fn convert_sensor_entity_with_unsupported_device_class_maps_to_custom() {
        let entity_id = "sensor.pressure_sensor".to_string();
        let state = "1013.25".to_string();
        let friendly_name = "Pressure Sensor";
        let unit = "hPa";

        let mut ha_attr = serde_json::Map::new();
        ha_attr.insert("friendly_name".to_string(), json!(friendly_name));
        ha_attr.insert("device_class".to_string(), json!("atmospheric_pressure"));
        ha_attr.insert("unit_of_measurement".to_string(), json!(unit));

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(friendly_name, entity.name.get("en").unwrap());
        assert_eq!(
            entity.device_class,
            Some(SensorDeviceClass::Custom.to_string())
        );
        let attributes = entity.attributes.unwrap();
        assert_eq!(
            Some(&json!("ON")),
            attributes.get(SensorAttribute::State.as_ref())
        );
        assert_eq!(
            Some(&json!(state)),
            attributes.get(SensorAttribute::Value.as_ref())
        );
        assert_eq!(
            Some(&json!(unit)),
            attributes.get(SensorAttribute::Unit.as_ref())
        );
    }
}
