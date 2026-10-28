// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Sensor entity-specific logic.

use crate::client::entity::number_to_json;
use crate::client::event::convert_ha_sensor_state;
use crate::client::model::EventData;
use crate::errors::ServiceError;
use serde_json::{Map, Value};
use std::collections::HashMap;
use uc_api::intg::AvailableIntgEntity;
use uc_api::{
    EntityType, SensorAttribute, SensorDeviceClass, SensorOptionField, intg::EntityChange,
};

/// HA sensor device classes with a non-numeric state.
const NON_NUMERIC_DEVICE_CLASSES: [&str; 4] = ["date", "enum", "timestamp", "uptime"];
/// HA sensor device classes with a numeric state: all `SensorDeviceClass` values except
/// [NON_NUMERIC_DEVICE_CLASSES]. Unknown, custom device classes are not numeric.
///
/// See <https://www.home-assistant.io/integrations/sensor/#device-class>
const NUMERIC_DEVICE_CLASSES: [&str; 58] = [
    "absolute_humidity",
    "apparent_power",
    "aqi",
    "area",
    "atmospheric_pressure",
    "battery",
    "blood_glucose_concentration",
    "carbon_monoxide",
    "carbon_dioxide",
    "conductivity",
    "current",
    "data_rate",
    "data_size",
    "distance",
    "duration",
    "energy",
    "energy_distance",
    "energy_storage",
    "frequency",
    "gas",
    "humidity",
    "illuminance",
    "irradiance",
    "moisture",
    "monetary",
    "nitrogen_dioxide",
    "nitrogen_monoxide",
    "nitrous_oxide",
    "ozone",
    "ph",
    "pm1",
    "pm10",
    "pm25",
    "pm4",
    "power_factor",
    "power",
    "precipitation",
    "precipitation_intensity",
    "pressure",
    "radon",
    "reactive_energy",
    "reactive_power",
    "signal_strength",
    "sound_pressure",
    "speed",
    "sulphur_dioxide",
    "temperature",
    "temperature_delta",
    "volatile_organic_compounds",
    "volatile_organic_compounds_parts",
    "voltage",
    "volume",
    "volume_storage",
    "volume_flow_rate",
    "water",
    "weight",
    "wind_direction",
    "wind_speed",
];
/// Maximum number of decimal places of the UC sensor `decimals` option.
const MAX_DECIMALS: u32 = 6;
/// Number of significant digits of a numeric sensor value. Every decimal number with up to 15
/// significant digits can be represented by a f64, the remaining digits are floating point
/// artifacts of calculated values.
const SIGNIFICANT_DIGITS: usize = 15;

/// Returns true if HA expects a numeric sensor state.
///
/// Same rules as `_numeric_state_expected` in HA's sensor entity: a sensor is numeric if it has a
/// state class, a unit of measurement, a display precision or a numeric device class. A sensor
/// with a non-numeric device class is never numeric, an unknown device class is ignored.
fn is_numeric_sensor(ha_attr: Option<&Map<String, Value>>, display_precision: Option<u32>) -> bool {
    let get = |key: &str| ha_attr.and_then(|a| a.get(key)).filter(|v| !v.is_null());
    let device_class = get("device_class").and_then(|v| v.as_str());
    if device_class.is_some_and(|class| NON_NUMERIC_DEVICE_CLASSES.contains(&class)) {
        return false;
    }
    get("state_class").is_some()
        || get("unit_of_measurement").is_some()
        || display_precision.is_some()
        || device_class.is_some_and(|class| NUMERIC_DEVICE_CLASSES.contains(&class))
}

/// Round a value to 15 significant digits to remove floating point artifacts, e.g.
/// `1.499999999999997` -> `1.5`.
fn round_significant(value: f64) -> f64 {
    format!("{:.*e}", SIGNIFICANT_DIGITS - 1, value)
        .parse()
        .unwrap_or(value)
}

/// Convert a HA sensor state to the UC sensor value.
///
/// - `unavailable` and `unknown`: `null` to clear the last value in the Remote.
/// - Numeric sensors: JSON number, if the state is a finite number. HA always sends the state as
///   text, but a text value isn't formatted by the Remote.
/// - Otherwise: the HA state as text.
fn convert_sensor_value(
    entity_id: &str,
    state: &str,
    ha_attr: Option<&Map<String, Value>>,
    display_precision: Option<u32>,
) -> Value {
    if matches!(state, "unavailable" | "unknown") {
        return Value::Null;
    }
    if !entity_id.starts_with("binary_sensor.")
        && is_numeric_sensor(ha_attr, display_precision)
        && let Ok(value) = state.parse::<f64>()
        && value.is_finite()
    {
        return number_to_json(round_significant(value));
    }
    state.into()
}

fn map_sensor_attributes(
    entity_id: &str,
    state: &str,
    ha_attr: Option<&mut Map<String, Value>>,
    display_precision: Option<u32>,
) -> Result<Map<String, Value>, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(2);
    attributes.insert(
        SensorAttribute::State.to_string(),
        convert_ha_sensor_state(state)?,
    );
    // HA: the state of a sensor entity is its currently detected value, which can be either text or a number.
    //     In addition, the entity can have the following states: unavailable, unknown
    //     https://www.home-assistant.io/integrations/sensor
    attributes.insert(
        SensorAttribute::Value.to_string(),
        convert_sensor_value(entity_id, state, ha_attr.as_deref(), display_precision),
    );

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

/// Convert a HA `state_changed` event of a sensor or binary sensor to an entity change event.
///
/// # Arguments
///
/// * `data`: event data
/// * `display_precision`: optional HA display precision of the sensor
pub(crate) fn sensor_event_to_entity_change(
    mut data: EventData,
    display_precision: Option<u32>,
) -> Result<EntityChange, ServiceError> {
    let attributes = map_sensor_attributes(
        &data.entity_id,
        &data.new_state.state,
        data.new_state.attributes.as_mut(),
        display_precision,
    )?;

    Ok(EntityChange {
        device_id: None, // prepared for device_id handling
        entity_type: EntityType::Sensor,
        entity_id: data.entity_id,
        attributes,
    })
}

/// Convert a HA sensor or binary sensor to an available UC sensor entity.
///
/// # Arguments
///
/// * `entity_id`: HA entity id
/// * `state`: HA state
/// * `ha_attr`: HA entity attributes
/// * `display_precision`: optional HA display precision of the sensor, set as `decimals` option.
pub(crate) fn convert_sensor_entity(
    entity_id: String,
    state: String,
    ha_attr: &mut Map<String, Value>,
    display_precision: Option<u32>,
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

    if let Some(decimals) = display_precision
        && !entity_id.starts_with("binary_sensor.")
    {
        options.insert(
            SensorOptionField::Decimals.to_string(),
            decimals.min(MAX_DECIMALS).into(),
        );
    }

    // convert attributes
    let attributes = Some(map_sensor_attributes(
        &entity_id,
        &state,
        Some(ha_attr),
        display_precision,
    )?);

    Ok(AvailableIntgEntity {
        entity_id,
        device_id: None, // prepared for device_id handling
        entity_type: EntityType::Sensor,
        device_class,
        name,
        icon: None,
        description: None,
        features: None,
        area: None,
        options: if options.is_empty() {
            None
        } else {
            Some(options)
        },
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

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr, None);

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
            Some(&json!(23.5)),
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

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr, None);

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

            let result =
                convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr, None);

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

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr, None);

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

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr, None);

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

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr, None);

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
            Some(&Value::Null),
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

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr, None);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();

        let attributes = entity.attributes.unwrap();
        assert_eq!(
            Some(&json!("UNKNOWN")),
            attributes.get(SensorAttribute::State.as_ref())
        );
        assert_eq!(
            Some(&Value::Null),
            attributes.get(SensorAttribute::Value.as_ref())
        );
    }

    #[test]
    fn convert_sensor_entity_binary_sensor_with_invalid_state_maps_to_on_state() {
        let entity_id = "binary_sensor.generic".to_string();
        let state = "off".to_string(); // sensors have no off state
        let mut ha_attr = serde_json::Map::new();
        ha_attr.insert("friendly_name".to_string(), json!("Generic Binary"));

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr, None);

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

        let result = convert_sensor_entity(entity_id.clone(), state.clone(), &mut ha_attr, None);

        assert!(result.is_ok(), "Expected converted entity: {result:?}");
        let entity = result.unwrap();
        assert_eq!(friendly_name, entity.name.get("en").unwrap());
        assert_eq!(
            entity.device_class,
            Some(SensorDeviceClass::Custom.to_string())
        );
        let options = entity.options.expect("custom sensor options expected");
        assert_eq!(
            Some(&json!("Atmospheric pressure")),
            options.get(SensorOptionField::CustomLabel.as_ref())
        );
        assert_eq!(
            Some(&json!(unit)),
            options.get(SensorOptionField::CustomUnit.as_ref())
        );
        let attributes = entity.attributes.unwrap();
        assert_eq!(
            Some(&json!("ON")),
            attributes.get(SensorAttribute::State.as_ref())
        );
        assert_eq!(
            Some(&json!(1013.25)),
            attributes.get(SensorAttribute::Value.as_ref())
        );
        assert_eq!(
            Some(&json!(unit)),
            attributes.get(SensorAttribute::Unit.as_ref())
        );
    }

    #[rstest::rstest]
    #[case(json!({}), None, false)]
    #[case(json!({ "friendly_name": "Firmware" }), None, false)]
    #[case(json!({ "unit_of_measurement": "kWh" }), None, true)]
    #[case(json!({ "state_class": "measurement" }), None, true)]
    #[case(json!({ "device_class": "power" }), None, true)]
    #[case(json!({ "device_class": null }), None, false)]
    #[case(json!({}), Some(2), true)]
    #[case(json!({ "device_class": "enum", "unit_of_measurement": "x" }), None, false)]
    #[case(json!({ "device_class": "timestamp", "state_class": "measurement" }), None, false)]
    #[case(json!({ "device_class": "date" }), Some(1), false)]
    #[case(json!({ "device_class": "uptime" }), None, false)]
    // unknown, custom device class
    #[case(json!({ "device_class": "firmware" }), None, false)]
    #[case(json!({ "device_class": "firmware", "unit_of_measurement": "x" }), None, true)]
    fn numeric_sensor_follows_ha_rules(
        #[case] attr: Value,
        #[case] display_precision: Option<u32>,
        #[case] expected: bool,
    ) {
        assert_eq!(
            expected,
            is_numeric_sensor(attr.as_object(), display_precision)
        );
    }

    #[rstest::rstest]
    #[case(1.499999999999997, 1.5)]
    #[case(0.30000000000000004, 0.3)]
    #[case(0.789266666666667, 0.789266666666667)]
    #[case(-21.4, -21.4)]
    #[case(123456789012.345, 123456789012.345)]
    #[case(0.0, 0.0)]
    fn round_to_significant_digits(#[case] value: f64, #[case] expected: f64) {
        assert_eq!(expected, round_significant(value));
    }

    #[rstest::rstest]
    // numeric sensor
    #[case("sensor.energy", "0.789266666666667", json!({ "unit_of_measurement": "kWh" }), json!(0.789266666666667))]
    #[case("sensor.calc", "1.499999999999997", json!({ "state_class": "measurement" }), json!(1.5))]
    #[case("sensor.temp", "23.0", json!({ "device_class": "temperature" }), json!(23))]
    #[case("sensor.temp", "-7", json!({ "device_class": "temperature" }), json!(-7))]
    #[case("sensor.big", "1.2345678901235e+17", json!({ "unit_of_measurement": "B" }), json!(1.2345678901235e17))]
    // numeric sensor without a numeric state
    #[case("sensor.temp", "n/a", json!({ "device_class": "temperature" }), json!("n/a"))]
    #[case("sensor.temp", "nan", json!({ "device_class": "temperature" }), json!("nan"))]
    #[case("sensor.temp", "inf", json!({ "device_class": "temperature" }), json!("inf"))]
    // text sensor: numeric looking text is kept
    #[case("sensor.firmware", "1.10", json!({}), json!("1.10"))]
    #[case("sensor.zip", "08001", json!({}), json!("08001"))]
    #[case("sensor.mode", "1", json!({ "device_class": "enum" }), json!("1"))]
    #[case("sensor.custom", "1.10", json!({ "device_class": "firmware" }), json!("1.10"))]
    #[case("binary_sensor.door", "on", json!({ "device_class": "door" }), json!("on"))]
    // no value
    #[case("sensor.temp", "unavailable", json!({ "device_class": "temperature" }), Value::Null)]
    #[case("sensor.temp", "unknown", json!({ "device_class": "temperature" }), Value::Null)]
    #[case("binary_sensor.door", "unavailable", json!({}), Value::Null)]
    fn sensor_value_conversion(
        #[case] entity_id: &str,
        #[case] state: &str,
        #[case] attr: Value,
        #[case] expected: Value,
    ) {
        assert_eq!(
            expected,
            convert_sensor_value(entity_id, state, attr.as_object(), None)
        );
    }

    #[test]
    fn sensor_value_with_display_precision_is_numeric() {
        assert_eq!(
            json!(42.5),
            convert_sensor_value("sensor.calc", "42.5", None, Some(1))
        );
    }

    #[rstest::rstest]
    #[case(Some(2), Some(json!(2)))]
    #[case(Some(0), Some(json!(0)))]
    #[case(Some(8), Some(json!(6)))]
    #[case(None, None)]
    fn convert_sensor_entity_sets_decimals_from_display_precision(
        #[case] display_precision: Option<u32>,
        #[case] expected: Option<Value>,
    ) {
        let mut ha_attr = json!({ "device_class": "energy", "unit_of_measurement": "kWh" })
            .as_object()
            .cloned()
            .unwrap();

        let entity = convert_sensor_entity(
            "sensor.energy".into(),
            "0.789266666666667".into(),
            &mut ha_attr,
            display_precision,
        )
        .expect("valid entity");

        assert_eq!(
            expected.as_ref(),
            entity
                .options
                .as_ref()
                .and_then(|o| o.get(SensorOptionField::Decimals.as_ref()))
        );
    }

    #[test]
    fn convert_binary_sensor_entity_ignores_display_precision() {
        let mut ha_attr = json!({ "device_class": "door" })
            .as_object()
            .cloned()
            .unwrap();

        let entity = convert_sensor_entity(
            "binary_sensor.door".into(),
            "on".into(),
            &mut ha_attr,
            Some(2),
        )
        .expect("valid entity");

        assert_eq!(None, entity.options);
    }

    #[test]
    fn sensor_event_to_entity_change_with_numeric_value() {
        let data: EventData = serde_json::from_value(json!({
            "entity_id": "sensor.power",
            "new_state": {
                "state": "1234.5",
                "attributes": { "device_class": "power", "unit_of_measurement": "W" }
            }
        }))
        .expect("valid EventData");

        let change = sensor_event_to_entity_change(data, Some(0)).expect("valid event");

        assert_eq!(
            json!({ "state": "ON", "value": 1234.5, "unit": "W" })
                .as_object()
                .cloned()
                .unwrap(),
            change.attributes
        );
    }
}
