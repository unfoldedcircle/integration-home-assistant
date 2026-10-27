// Copyright (c) 2026 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Number-entity specific logic for the HA `number`, `input_number` and `counter` entities.
//!
//! The UC number entity options (min, max, step) are static. They are taken from the HA entity
//! attributes and cached in [NumberState], which is used for clamping and rounding command values
//! and for calculating `increment` / `decrement` commands of entities without a native service.

use log::warn;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uc_api::intg::{AvailableIntgEntity, EntityChange};
use uc_api::{
    EntityType, NumberAttribute, NumberDeviceClass, NumberDisplayMode, NumberFeature,
    NumberOptionField,
};

use crate::client::model::EventData;
use crate::errors::ServiceError;

/// Smallest supported step size of the UC number entity.
pub(crate) const MIN_STEP: f64 = 0.000001;
/// Maximum number of decimal places of the UC number entity. Matches the smallest step size.
const MAX_DECIMALS: u32 = 6;
/// Duration how long an expected value after a command is used for calculating the next
/// `increment` or `decrement` command, while waiting for the HA state update.
pub(crate) const OPTIMISTIC_VALUE_TIMEOUT: Duration = Duration::from_secs(2);

/// Value range of a number entity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NumberRange {
    /// Minimum value, `None`: no lower limit.
    pub min: Option<f64>,
    /// Maximum value, `None`: no upper limit.
    pub max: Option<f64>,
    /// UC step size: HA step size, limited to [MIN_STEP].
    pub step: f64,
    /// Original HA step size.
    pub ha_step: f64,
    /// Number of decimal places to show in the UI.
    pub decimals: u32,
}

impl NumberRange {
    /// Create a value range from HA entity attributes.
    ///
    /// - `number` and `input_number` use the `min`, `max` and `step` attributes.
    /// - `counter` uses the optional `minimum` and `maximum` attributes, and `step`. A missing
    ///   attribute means no limit.
    fn from_ha_attributes(domain: &str, ha_attr: Option<&Map<String, Value>>) -> Self {
        let get = |key: &str| {
            ha_attr
                .and_then(|attr| attr.get(key))
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite())
        };

        let (min, max) = if domain == "counter" {
            (get("minimum"), get("maximum"))
        } else {
            (
                Some(get("min").unwrap_or(0.0)),
                Some(get("max").unwrap_or(100.0)),
            )
        };
        let ha_step = get("step").filter(|v| *v > 0.0).unwrap_or(1.0);
        let step = ha_step.max(MIN_STEP);

        Self {
            min,
            max,
            step,
            ha_step,
            decimals: decimals_for(&[step, min.unwrap_or_default()]),
        }
    }

    /// Returns true if the HA step size is smaller than the supported UC step size.
    pub fn is_step_limited(&self) -> bool {
        self.ha_step < self.step
    }

    /// Clamp a value to the min and max range.
    pub fn clamp(&self, value: f64) -> f64 {
        // f64::clamp panics if min > max
        let value = self.min.map_or(value, |min| value.max(min));
        self.max.map_or(value, |max| value.min(max))
    }

    /// Round a value to the step size and clamp it to the value range.
    ///
    /// - Counter: value is rounded to an integer multiple of the step size, as required by the HA
    ///   `counter.set_value` service.
    /// - Otherwise: value is rounded to the step grid starting at `min` (or 0 without lower limit)
    ///   and to the number of decimal places to remove floating point artifacts.
    pub fn normalize(&self, value: f64, counter: bool) -> f64 {
        if counter {
            let step = self.ha_step.round().max(1.0);
            let value = (value / step).round() * step;
            let value = self
                .min
                .map_or(value, |min| value.max((min / step).ceil() * step));
            self.max
                .map_or(value, |max| value.min((max / step).floor() * step))
        } else {
            let origin = self.min.unwrap_or_default();
            let value = origin + ((value - origin) / self.step).round() * self.step;
            round_to(self.clamp(value), self.decimals)
        }
    }
}

/// Cached state of a number entity for command processing.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NumberState {
    pub range: NumberRange,
    /// Last known value reported by HA.
    pub value: Option<f64>,
    /// Expected value after the last command and the command timestamp.
    pub optimistic: Option<(f64, Instant)>,
}

impl NumberState {
    /// Get the current value for calculating a relative value change.
    ///
    /// The expected value of the last command is used if it is not older than
    /// [OPTIMISTIC_VALUE_TIMEOUT]. This prevents lost steps with quickly repeated commands, before
    /// HA reports the new value. Otherwise, the last known value from HA is used.
    pub fn current_value(&self, now: Instant) -> Option<f64> {
        match self.optimistic {
            Some((value, timestamp))
                if now.duration_since(timestamp) < OPTIMISTIC_VALUE_TIMEOUT =>
            {
                Some(value)
            }
            _ => self.value,
        }
    }
}

/// Parsed HA number entity.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NumberEntityInfo {
    pub range: NumberRange,
    pub value: Option<f64>,
    /// UC entity state: ON, UNAVAILABLE, UNKNOWN
    pub state: &'static str,
    pub unit: Option<String>,
    pub device_class: Option<NumberDeviceClass>,
    pub display_mode: Option<NumberDisplayMode>,
}

impl From<&NumberEntityInfo> for NumberState {
    fn from(info: &NumberEntityInfo) -> Self {
        Self {
            range: info.range,
            value: info.value,
            optimistic: None,
        }
    }
}

/// Parse a HA `number`, `input_number` or `counter` entity.
pub(crate) fn parse_number_entity(
    entity_id: &str,
    state: &str,
    ha_attr: Option<&Map<String, Value>>,
) -> NumberEntityInfo {
    let domain = entity_id
        .split_once('.')
        .map(|(d, _)| d)
        .unwrap_or_default();
    let range = NumberRange::from_ha_attributes(domain, ha_attr);

    let (state, value) = match state {
        "unavailable" => ("UNAVAILABLE", None),
        "unknown" => ("UNKNOWN", None),
        _ => match state.parse::<f64>() {
            Ok(value) if value.is_finite() => ("ON", Some(value)),
            _ => {
                warn!("Invalid number entity state of {entity_id}: {state}");
                ("UNKNOWN", None)
            }
        },
    };

    let get_str = |key: &str| {
        ha_attr
            .and_then(|attr| attr.get(key))
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
    };
    let unit = get_str("unit_of_measurement");

    NumberEntityInfo {
        range,
        value,
        state,
        unit: unit.map(|v| v.to_string()),
        device_class: get_str("device_class").and_then(|v| convert_device_class(v, unit)),
        display_mode: get_str("mode").and_then(convert_display_mode),
    }
}

/// Convert a HA number device class to a UC number device class.
///
/// HA device classes without a matching UC device class are not mapped. The entity is then a
/// generic number with the HA unit.
///
/// - HA `sound_pressure` is a measured sound level and not mapped to the UC `audio_level`.
/// - `duration` is only mapped with a unit supported by the UC `duration` device class.
///
/// See <https://www.home-assistant.io/integrations/number/#device-class>
fn convert_device_class(device_class: &str, unit: Option<&str>) -> Option<NumberDeviceClass> {
    match device_class {
        "battery" => Some(NumberDeviceClass::Battery),
        "current" => Some(NumberDeviceClass::Current),
        "duration" => match unit {
            None | Some("ms" | "s" | "min" | "h" | "d") => Some(NumberDeviceClass::Duration),
            Some(_) => None,
        },
        "energy" => Some(NumberDeviceClass::Energy),
        "humidity" => Some(NumberDeviceClass::Humidity),
        "illuminance" => Some(NumberDeviceClass::Illuminance),
        "power" => Some(NumberDeviceClass::Power),
        "temperature" => Some(NumberDeviceClass::Temperature),
        "voltage" => Some(NumberDeviceClass::Voltage),
        _ => None,
    }
}

/// Convert the HA `mode` attribute of a `number` or `input_number` entity to a UC display mode.
fn convert_display_mode(mode: &str) -> Option<NumberDisplayMode> {
    match mode {
        "auto" => Some(NumberDisplayMode::Auto),
        "slider" => Some(NumberDisplayMode::Slider),
        "box" => Some(NumberDisplayMode::Stepper),
        _ => None,
    }
}

/// Number of decimal places required to represent the given values, limited to [MAX_DECIMALS].
pub(crate) fn decimals_for(values: &[f64]) -> u32 {
    values
        .iter()
        .map(|value| {
            (0..MAX_DECIMALS)
                .find(|decimals| {
                    let scaled = value * 10f64.powi(*decimals as i32);
                    // relative tolerance for floating point artifacts, e.g. 0.1 * 3
                    (scaled - scaled.round()).abs() < 1e-9 * scaled.abs().max(1.0)
                })
                .unwrap_or(MAX_DECIMALS)
        })
        .max()
        .unwrap_or_default()
}

/// Round a value to the given number of decimal places.
pub(crate) fn round_to(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).round() / factor
}

/// Convert a number to a JSON value. Whole numbers are converted to an integer value.
pub(crate) fn number_to_json(value: f64) -> Value {
    // largest integer which can be represented exactly in a f64
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    if value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER {
        Value::from(value as i64)
    } else {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}

fn map_number_attributes(info: &NumberEntityInfo) -> Map<String, Value> {
    let mut attributes = Map::with_capacity(2);
    attributes.insert(NumberAttribute::State.to_string(), info.state.into());
    // null clears the last value in the Remote while unavailable or unknown
    attributes.insert(
        NumberAttribute::Value.to_string(),
        info.value.map(number_to_json).unwrap_or(Value::Null),
    );
    attributes
}

/// Convert a HA `state_changed` event to an entity change event.
///
/// Returns the entity change event and the new number state to update the cached state.
pub(crate) fn number_event_to_entity_change(
    data: EventData,
) -> Result<(EntityChange, NumberState), ServiceError> {
    let info = parse_number_entity(
        &data.entity_id,
        &data.new_state.state,
        data.new_state.attributes.as_ref(),
    );

    Ok((
        EntityChange {
            device_id: None,
            entity_type: EntityType::Number,
            entity_id: data.entity_id,
            attributes: map_number_attributes(&info),
        },
        NumberState::from(&info),
    ))
}

/// Convert a HA `number`, `input_number` or `counter` entity to an available UC number entity.
///
/// Returns the available entity and the number state to initialize the cached state.
pub(crate) fn convert_number_entity(
    entity_id: String,
    state: String,
    ha_attr: &mut Map<String, Value>,
) -> Result<(AvailableIntgEntity, NumberState), ServiceError> {
    let friendly_name = ha_attr.get("friendly_name").and_then(|v| v.as_str());
    let name = HashMap::from([("en".into(), friendly_name.unwrap_or(&entity_id).into())]);

    let info = parse_number_entity(&entity_id, &state, Some(ha_attr));
    let range = &info.range;
    if range.is_step_limited() {
        warn!(
            "Step size {} of {entity_id} is smaller than the supported minimum, using {}",
            range.ha_step, range.step
        );
    }

    let mut options = Map::with_capacity(6);
    // An unbounded range must be set explicitly with null, otherwise the default value is used
    options.insert(
        NumberOptionField::Min.to_string(),
        range.min.map(number_to_json).unwrap_or(Value::Null),
    );
    options.insert(
        NumberOptionField::Max.to_string(),
        range.max.map(number_to_json).unwrap_or(Value::Null),
    );
    options.insert(
        NumberOptionField::Step.to_string(),
        number_to_json(range.step),
    );
    options.insert(
        NumberOptionField::Decimals.to_string(),
        range.decimals.into(),
    );
    if let Some(unit) = &info.unit {
        options.insert(NumberOptionField::Unit.to_string(), unit.as_str().into());
    }
    if let Some(display_mode) = info.display_mode {
        options.insert(
            NumberOptionField::DisplayMode.to_string(),
            display_mode.as_ref().into(),
        );
    }

    let attributes = Some(map_number_attributes(&info));
    let number_state = NumberState::from(&info);

    Ok((
        AvailableIntgEntity {
            entity_id,
            device_id: None,
            entity_type: EntityType::Number,
            device_class: info.device_class.map(|v| v.to_string()),
            name,
            icon: None,
            description: None,
            features: Some(vec![NumberFeature::Value.to_string()]),
            area: None,
            options: Some(options),
            attributes,
        },
        number_state,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use serde_json::json;

    fn ha_attributes(attr: Value) -> Map<String, Value> {
        attr.as_object().expect("attributes object").clone()
    }

    fn event_data(entity_id: &str, state: &str, attr: Value) -> EventData {
        serde_json::from_value(json!({
            "entity_id": entity_id,
            "new_state": {
                "state": state,
                "attributes": attr
            }
        }))
        .expect("valid EventData")
    }

    fn range(min: f64, max: f64, step: f64) -> NumberRange {
        NumberRange {
            min: Some(min),
            max: Some(max),
            step: step.max(MIN_STEP),
            ha_step: step,
            decimals: decimals_for(&[step.max(MIN_STEP), min]),
        }
    }

    fn unbounded_range(min: Option<f64>, max: Option<f64>, step: f64) -> NumberRange {
        NumberRange {
            min,
            max,
            ..range(min.unwrap_or_default(), 0.0, step)
        }
    }

    #[test]
    fn convert_number_entity_with_float_state() {
        let mut attr = ha_attributes(json!({
            "min": 0, "max": 100, "step": 1.0, "mode": "auto",
            "unit_of_measurement": "%", "friendly_name": "Rocky Lautstärke"
        }));

        let (entity, state) =
            convert_number_entity("number.rocky_volume".into(), "65.0".into(), &mut attr)
                .expect("valid entity");

        assert_eq!("number.rocky_volume", entity.entity_id);
        assert_eq!(EntityType::Number, entity.entity_type);
        assert_eq!(Some(&"Rocky Lautstärke".to_string()), entity.name.get("en"));
        assert_eq!(Some(vec!["value".to_string()]), entity.features);
        assert_eq!(None, entity.icon);
        assert_eq!(
            Some(ha_attributes(json!({
                "min": 0, "max": 100, "step": 1, "decimals": 0, "unit": "%", "display_mode": "auto"
            }))),
            entity.options
        );
        assert_eq!(
            Some(ha_attributes(json!({ "state": "ON", "value": 65 }))),
            entity.attributes
        );
        assert_eq!(Some(65.0), state.value);
        assert_eq!(range(0.0, 100.0, 1.0), state.range);
        assert_eq!(None, state.optimistic);
    }

    #[test]
    fn convert_unavailable_number_entity_without_unit() {
        let mut attr = ha_attributes(json!({
            "min": -10, "max": 10, "step": 1.0, "mode": "auto", "friendly_name": "Esstisch Bass"
        }));

        let (entity, state) = convert_number_entity(
            "number.esstisch_bass".into(),
            "unavailable".into(),
            &mut attr,
        )
        .expect("valid entity");

        assert_eq!(
            Some(ha_attributes(json!({
                "min": -10, "max": 10, "step": 1, "decimals": 0, "display_mode": "auto"
            }))),
            entity.options
        );
        assert_eq!(
            Some(ha_attributes(
                json!({ "state": "UNAVAILABLE", "value": null })
            )),
            entity.attributes
        );
        assert_eq!(None, state.value);
    }

    #[test]
    fn convert_number_entity_without_friendly_name_uses_entity_id() {
        let mut attr = ha_attributes(json!({ "min": 0, "max": 10, "step": 1 }));

        let (entity, _) = convert_number_entity("number.test".into(), "1".into(), &mut attr)
            .expect("valid entity");

        assert_eq!(Some(&"number.test".to_string()), entity.name.get("en"));
    }

    #[test]
    fn convert_input_number_with_half_steps() {
        let mut attr = ha_attributes(json!({
            "initial": null, "editable": true, "min": -5.0, "max": 5.0, "step": 0.5,
            "mode": "slider", "unit_of_measurement": "dB", "friendly_name": "Bass"
        }));

        let (entity, state) =
            convert_number_entity("input_number.bass".into(), "1.5".into(), &mut attr)
                .expect("valid entity");

        assert_eq!(
            Some(ha_attributes(json!({
                "min": -5, "max": 5, "step": 0.5, "decimals": 1, "unit": "dB", "display_mode": "slider"
            }))),
            entity.options
        );
        assert_eq!(
            Some(ha_attributes(json!({ "state": "ON", "value": 1.5 }))),
            entity.attributes
        );
        assert_eq!(Some(1.5), state.value);
    }

    #[test]
    fn convert_number_entity_with_fine_step() {
        let mut attr = ha_attributes(json!({ "min": 0, "max": 1, "step": 0.001 }));

        let (entity, state) =
            convert_number_entity("number.fine".into(), "0.123".into(), &mut attr)
                .expect("valid entity");

        let options = entity.options.expect("options");
        assert_eq!(Some(&json!(0.001)), options.get("step"));
        assert_eq!(Some(&json!(3)), options.get("decimals"));
        assert!(!state.range.is_step_limited());
    }

    #[test]
    fn convert_number_entity_with_too_small_step_is_limited() {
        let mut attr = ha_attributes(json!({ "min": 0, "max": 1, "step": 0.0000001 }));

        let (entity, state) =
            convert_number_entity("number.fine".into(), "0.1234567".into(), &mut attr)
                .expect("valid entity");

        let options = entity.options.expect("options");
        assert_eq!(Some(&json!(0.000001)), options.get("step"));
        assert_eq!(Some(&json!(6)), options.get("decimals"));
        assert!(state.range.is_step_limited());
        assert_eq!(0.0000001, state.range.ha_step);
        // value is not rounded
        assert_eq!(
            Some(ha_attributes(json!({ "state": "ON", "value": 0.1234567 }))),
            entity.attributes
        );
    }

    #[test]
    fn convert_number_entity_without_range_attributes_uses_ha_defaults() {
        let mut attr = ha_attributes(json!({}));

        let (entity, _) = convert_number_entity("number.test".into(), "5".into(), &mut attr)
            .expect("valid entity");

        assert_eq!(
            Some(ha_attributes(json!({
                "min": 0, "max": 100, "step": 1, "decimals": 0
            }))),
            entity.options
        );
    }

    #[test]
    fn convert_bounded_counter() {
        let mut attr = ha_attributes(json!({
            "initial": 0, "editable": true, "step": 2, "minimum": 0, "maximum": 10,
            "friendly_name": "Coffee cups"
        }));

        let (entity, state) = convert_number_entity("counter.coffee".into(), "4".into(), &mut attr)
            .expect("valid entity");

        assert_eq!(
            Some(ha_attributes(json!({
                "min": 0, "max": 10, "step": 2, "decimals": 0
            }))),
            entity.options
        );
        assert_eq!(
            Some(ha_attributes(json!({ "state": "ON", "value": 4 }))),
            entity.attributes
        );
        assert_eq!(Some(4.0), state.value);
    }

    #[rstest]
    #[case(json!({ "initial": 0, "step": 1 }), Value::Null, Value::Null)]
    #[case(json!({ "initial": 0, "step": 1, "minimum": null, "maximum": null }), Value::Null, Value::Null)]
    #[case(json!({ "initial": 0, "step": 1, "minimum": 0 }), json!(0), Value::Null)]
    #[case(json!({ "initial": 0, "step": 1, "maximum": 20 }), Value::Null, json!(20))]
    fn convert_unbounded_counter_sets_null_limit(
        #[case] attr: Value,
        #[case] min: Value,
        #[case] max: Value,
    ) {
        let mut attr = ha_attributes(attr);

        let (entity, _) = convert_number_entity("counter.test".into(), "3".into(), &mut attr)
            .expect("valid entity");

        let options = entity.options.expect("options");
        assert_eq!(Some(&min), options.get("min"));
        assert_eq!(Some(&max), options.get("max"));
        assert_eq!(None, options.get("display_mode"));
        assert_eq!(None, entity.device_class);
    }

    #[test]
    fn counter_ignores_min_max_attributes_of_number_entities() {
        let attr = ha_attributes(json!({ "min": 5, "max": 6, "step": 1 }));

        let info = parse_number_entity("counter.test", "3", Some(&attr));

        assert_eq!(None, info.range.min);
        assert_eq!(None, info.range.max);
    }

    #[rstest]
    #[case("battery", Some("%"), Some("battery"))]
    #[case("current", Some("A"), Some("current"))]
    #[case("duration", Some("min"), Some("duration"))]
    #[case("duration", Some("s"), Some("duration"))]
    #[case("duration", None, Some("duration"))]
    #[case("duration", Some("μs"), None)]
    #[case("energy", Some("Wh"), Some("energy"))]
    #[case("humidity", Some("%"), Some("humidity"))]
    #[case("illuminance", Some("lx"), Some("illuminance"))]
    #[case("power", Some("kW"), Some("power"))]
    #[case("temperature", Some("°F"), Some("temperature"))]
    #[case("voltage", Some("V"), Some("voltage"))]
    // measured sound level, not an audio setting
    #[case("sound_pressure", Some("dB"), None)]
    // liquid volume, not audio volume
    #[case("volume", Some("L"), None)]
    #[case("distance", Some("cm"), None)]
    #[case("", None, None)]
    fn convert_number_device_class(
        #[case] ha_device_class: &str,
        #[case] unit: Option<&str>,
        #[case] expected: Option<&str>,
        #[values("number", "input_number")] domain: &str,
    ) {
        let mut attr = ha_attributes(json!({
            "min": 0, "max": 100, "step": 1, "device_class": ha_device_class,
            "unit_of_measurement": unit
        }));

        let (entity, _) = convert_number_entity(format!("{domain}.test"), "5".into(), &mut attr)
            .expect("valid entity");

        assert_eq!(expected.map(|v| v.to_string()), entity.device_class);
        // the HA unit is always used
        let options = entity.options.expect("options");
        assert_eq!(unit.map(|v| json!(v)).as_ref(), options.get("unit"));
    }

    #[rstest]
    #[case(json!("auto"), Some("auto"))]
    #[case(json!("slider"), Some("slider"))]
    #[case(json!("box"), Some("stepper"))]
    #[case(json!("foobar"), None)]
    #[case(Value::Null, None)]
    fn convert_number_display_mode(
        #[case] mode: Value,
        #[case] expected: Option<&str>,
        #[values("number", "input_number")] domain: &str,
    ) {
        let mut attr = ha_attributes(json!({ "min": 0, "max": 100, "step": 1, "mode": mode }));

        let (entity, _) = convert_number_entity(format!("{domain}.test"), "5".into(), &mut attr)
            .expect("valid entity");

        let options = entity.options.expect("options");
        assert_eq!(
            expected.map(|v| json!(v)).as_ref(),
            options.get("display_mode")
        );
    }

    #[rstest]
    #[case("0", "ON", Some(0.0))]
    #[case("23.0", "ON", Some(23.0))]
    #[case("-7.5", "ON", Some(-7.5))]
    #[case("0.3", "ON", Some(0.3))]
    #[case("1.2345678901235e+17", "ON", Some(1.2345678901235e17))]
    #[case("unavailable", "UNAVAILABLE", None)]
    #[case("unknown", "UNKNOWN", None)]
    #[case("foobar", "UNKNOWN", None)]
    #[case("", "UNKNOWN", None)]
    #[case("NaN", "UNKNOWN", None)]
    #[case("inf", "UNKNOWN", None)]
    fn parse_number_state(
        #[case] ha_state: &str,
        #[case] uc_state: &str,
        #[case] value: Option<f64>,
    ) {
        let info = parse_number_entity("number.test", ha_state, None);

        assert_eq!(uc_state, info.state);
        assert_eq!(value, info.value);
    }

    #[rstest]
    #[case(&[1.0], 0)]
    #[case(&[1.0, 0.0], 0)]
    #[case(&[0.5], 1)]
    #[case(&[0.1], 1)]
    #[case(&[0.25], 2)]
    #[case(&[0.01], 2)]
    #[case(&[0.001], 3)]
    #[case(&[0.000001], 6)]
    #[case(&[0.0000001], 6)]
    #[case(&[0.1234567], 6)]
    #[case(&[1.0, 0.5], 1)]
    #[case(&[0.5, -10.25], 2)]
    #[case(&[3.0, -1_000_000.0], 0)]
    #[case(&[1_000_000.5], 1)]
    #[case(&[], 0)]
    fn decimals(#[case] values: &[f64], #[case] expected: u32) {
        assert_eq!(expected, decimals_for(values));
    }

    #[rstest]
    #[case(0.0, json!(0))]
    #[case(65.0, json!(65))]
    #[case(-10.0, json!(-10))]
    #[case(1.5, json!(1.5))]
    #[case(0.01, json!(0.01))]
    #[case(1.0e20, json!(1.0e20))]
    fn number_json_value(#[case] value: f64, #[case] expected: Value) {
        assert_eq!(expected, number_to_json(value));
    }

    #[rstest]
    #[case("number.test")]
    #[case("input_number.test")]
    #[case("counter.test")]
    fn number_event_to_entity_change_with_value(#[case] entity_id: &str) {
        let data = event_data(
            entity_id,
            "7",
            json!({ "min": 0, "max": 10, "minimum": 0, "maximum": 10, "step": 1 }),
        );

        let (change, state) = number_event_to_entity_change(data).expect("valid event");

        assert_eq!(EntityType::Number, change.entity_type);
        assert_eq!(entity_id, change.entity_id);
        assert_eq!(
            ha_attributes(json!({ "state": "ON", "value": 7 })),
            change.attributes
        );
        assert_eq!(Some(7.0), state.value);
        assert_eq!(range(0.0, 10.0, 1.0), state.range);
    }

    #[test]
    fn number_event_to_entity_change_from_ha_event() {
        let full_event_json = json!({
          "type": "event",
          "event": {
            "event_type": "state_changed",
            "data": {
              "entity_id": "input_number.scene_intensity",
              "old_state": {
                "entity_id": "input_number.scene_intensity",
                "state": "40.0",
                "attributes": {
                  "initial": null, "editable": true, "min": 0.0, "max": 100.0, "step": 5.0,
                  "mode": "slider", "unit_of_measurement": "%", "friendly_name": "Scene intensity"
                }
              },
              "new_state": {
                "entity_id": "input_number.scene_intensity",
                "state": "45.0",
                "attributes": {
                  "initial": null, "editable": true, "min": 0.0, "max": 100.0, "step": 5.0,
                  "mode": "slider", "unit_of_measurement": "%", "friendly_name": "Scene intensity"
                },
                "last_changed": "2026-10-25T10:30:50.490383+00:00",
                "last_reported": "2026-10-25T10:30:50.491155+00:00",
                "last_updated": "2026-10-25T10:30:50.490383+00:00",
                "context": { "id": "01KDEG0AFTND3FZH5MM6SD97EA", "parent_id": null, "user_id": null }
              }
            },
            "origin": "LOCAL",
            "time_fired": "2026-10-25T10:30:50.490383+00:00",
            "context": { "id": "01KDEG0AFTND3FZH5MM6SD97EA", "parent_id": null, "user_id": null }
          },
          "id": 2
        });
        let data: EventData = serde_json::from_value(
            full_event_json
                .pointer("/event/data")
                .expect("event data")
                .clone(),
        )
        .expect("valid EventData");

        let (change, state) = number_event_to_entity_change(data).expect("valid event");

        assert_eq!("input_number.scene_intensity", change.entity_id);
        assert_eq!(
            ha_attributes(json!({ "state": "ON", "value": 45 })),
            change.attributes
        );
        assert_eq!(range(0.0, 100.0, 5.0), state.range);
    }

    #[test]
    fn number_event_to_entity_change_unavailable() {
        let data = event_data("number.test", "unavailable", json!({ "min": 0, "max": 10 }));

        let (change, state) = number_event_to_entity_change(data).expect("valid event");

        assert_eq!(
            ha_attributes(json!({ "state": "UNAVAILABLE", "value": null })),
            change.attributes
        );
        assert_eq!(None, state.value);
    }

    #[rstest]
    // regular step grid starting at min
    #[case(range(0.0, 100.0, 1.0), 42.4, 42.0)]
    #[case(range(0.0, 100.0, 1.0), 42.5, 43.0)]
    #[case(range(0.0, 100.0, 5.0), 42.0, 40.0)]
    #[case(range(0.0, 100.0, 5.0), 43.0, 45.0)]
    #[case(range(0.5, 10.5, 1.0), 3.0, 3.5)]
    #[case(range(-5.0, 5.0, 0.5), 1.3, 1.5)]
    #[case(range(0.0, 1.0, 0.1), 0.30000000000000004, 0.3)]
    #[case(range(0.0, 1.0, 0.001), 0.123, 0.123)]
    #[case(range(0.0, 1.0, 0.001), 0.1234, 0.123)]
    #[case(range(0.0, 1.0, 0.0000001), 0.12345678, 0.123457)]
    // clamping
    #[case(range(0.0, 100.0, 1.0), 101.0, 100.0)]
    #[case(range(0.0, 100.0, 1.0), -1.0, 0.0)]
    #[case(range(0.0, 10.0, 3.0), 10.0, 9.0)]
    #[case(range(0.0, 10.0, 3.0), 11.0, 10.0)]
    #[case(range(-10.0, 10.0, 1.0), -20.0, -10.0)]
    // unbounded: step grid starts at 0
    #[case(unbounded_range(None, None, 0.5), -1234.3, -1234.5)]
    #[case(unbounded_range(None, Some(10.0), 1.0), 20.0, 10.0)]
    #[case(unbounded_range(Some(0.5), None, 1.0), 1000.2, 1000.5)]
    fn normalize_number_value(
        #[case] range: NumberRange,
        #[case] value: f64,
        #[case] expected: f64,
    ) {
        assert_eq!(expected, range.normalize(value, false));
    }

    #[rstest]
    #[case(range(0.0, 10.0, 1.0), 4.4, 4.0)]
    #[case(range(0.0, 10.0, 1.0), 4.5, 5.0)]
    #[case(range(0.0, 10.0, 2.0), 5.0, 6.0)]
    #[case(range(0.0, 10.0, 2.0), 4.9, 4.0)]
    // multiples of step within the bounds
    #[case(range(1.0, 9.0, 2.0), 0.0, 2.0)]
    #[case(range(1.0, 9.0, 2.0), 10.0, 8.0)]
    // unbounded
    #[case(unbounded_range(None, None, 1.0), -3.0, -3.0)]
    #[case(unbounded_range(None, None, 5.0), 1_000_003.0, 1_000_005.0)]
    #[case(unbounded_range(Some(0.0), None, 1.0), -3.0, 0.0)]
    #[case(unbounded_range(None, Some(9.0), 2.0), 12.0, 8.0)]
    fn normalize_counter_value(
        #[case] range: NumberRange,
        #[case] value: f64,
        #[case] expected: f64,
    ) {
        assert_eq!(expected, range.normalize(value, true));
    }

    #[test]
    fn clamp_with_invalid_range_does_not_panic() {
        let range = range(10.0, 0.0, 1.0);
        assert_eq!(0.0, range.clamp(5.0));
        assert_eq!(0.0, range.normalize(5.0, false));
        assert_eq!(0.0, range.normalize(5.0, true));
    }

    #[test]
    fn current_value_uses_recent_optimistic_value() {
        let now = Instant::now();
        let state = NumberState {
            range: range(0.0, 100.0, 1.0),
            value: Some(10.0),
            optimistic: Some((12.0, now)),
        };

        assert_eq!(Some(12.0), state.current_value(now));
        assert_eq!(
            Some(12.0),
            state.current_value(now + OPTIMISTIC_VALUE_TIMEOUT - Duration::from_millis(1))
        );
        assert_eq!(
            Some(10.0),
            state.current_value(now + OPTIMISTIC_VALUE_TIMEOUT)
        );
    }

    #[test]
    fn current_value_without_optimistic_value() {
        let state = NumberState {
            range: range(0.0, 100.0, 1.0),
            value: None,
            optimistic: None,
        };

        assert_eq!(None, state.current_value(Instant::now()));
    }
}
