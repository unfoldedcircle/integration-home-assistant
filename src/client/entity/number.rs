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
use uc_api::{EntityType, NumberAttribute, NumberFeature, NumberOptionField};

use crate::client::model::EventData;
use crate::errors::ServiceError;

/// Fallback limit for the min and max option of a HA counter without a minimum or maximum.
///
/// The large range makes the Remote show a value field with increment and decrement buttons
/// instead of a slider.
pub(crate) const COUNTER_UNBOUNDED_LIMIT: f64 = 1_000_000.0;
/// Smallest supported step size of the UC number entity.
pub(crate) const MIN_STEP: f64 = 0.01;
/// Maximum number of decimal places. Matches the smallest supported step size.
const MAX_DECIMALS: u32 = 2;
/// Duration how long an expected value after a command is used for calculating the next
/// `increment` or `decrement` command, while waiting for the HA state update.
pub(crate) const OPTIMISTIC_VALUE_TIMEOUT: Duration = Duration::from_secs(2);

/// Value range of a number entity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NumberRange {
    pub min: f64,
    pub max: f64,
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
    /// - `counter` uses the optional `minimum` and `maximum` attributes, and `step`.
    fn from_ha_attributes(domain: &str, ha_attr: Option<&Map<String, Value>>) -> Self {
        let get = |key: &str| {
            ha_attr
                .and_then(|attr| attr.get(key))
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite())
        };

        let (min, max) = if domain == "counter" {
            (
                get("minimum").unwrap_or(-COUNTER_UNBOUNDED_LIMIT),
                get("maximum").unwrap_or(COUNTER_UNBOUNDED_LIMIT),
            )
        } else {
            (get("min").unwrap_or(0.0), get("max").unwrap_or(100.0))
        };
        let ha_step = get("step").filter(|v| *v > 0.0).unwrap_or(1.0);
        let step = ha_step.max(MIN_STEP);

        Self {
            min,
            max,
            step,
            ha_step,
            decimals: decimals_for(&[step, min]),
        }
    }

    /// Returns true if the HA step size is smaller than the supported UC step size.
    pub fn is_step_limited(&self) -> bool {
        self.ha_step < self.step
    }

    /// Clamp a value to the min and max range.
    pub fn clamp(&self, value: f64) -> f64 {
        // f64::clamp panics if min > max
        value.max(self.min).min(self.max)
    }

    /// Round a value to the step size and clamp it to the value range.
    ///
    /// - Counter: value is rounded to an integer multiple of the step size, as required by the HA
    ///   `counter.set_value` service.
    /// - Otherwise: value is rounded to the step grid starting at `min` and to the number of
    ///   decimal places to remove floating point artifacts.
    pub fn normalize(&self, value: f64, counter: bool) -> f64 {
        if counter {
            let step = self.ha_step.round().max(1.0);
            let min = (self.min / step).ceil() * step;
            let max = (self.max / step).floor() * step;
            ((value / step).round() * step).max(min).min(max)
        } else {
            let value = self.min + ((value - self.min) / self.step).round() * self.step;
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

    let unit = ha_attr
        .and_then(|attr| attr.get("unit_of_measurement"))
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string());

    NumberEntityInfo {
        range,
        value,
        state,
        unit,
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
                    (scaled - scaled.round()).abs() < 1e-6
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
    if let Some(value) = info.value {
        attributes.insert(NumberAttribute::Value.to_string(), number_to_json(value));
    }
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

    let mut options = Map::with_capacity(5);
    options.insert(
        NumberOptionField::Min.to_string(),
        number_to_json(range.min),
    );
    options.insert(
        NumberOptionField::Max.to_string(),
        number_to_json(range.max),
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

    let attributes = Some(map_number_attributes(&info));
    let number_state = NumberState::from(&info);

    Ok((
        AvailableIntgEntity {
            entity_id,
            device_id: None,
            entity_type: EntityType::Number,
            device_class: None,
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
            min,
            max,
            step: step.max(MIN_STEP),
            ha_step: step,
            decimals: decimals_for(&[step.max(MIN_STEP), min]),
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
                "min": 0, "max": 100, "step": 1, "decimals": 0, "unit": "%"
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
                "min": -10, "max": 10, "step": 1, "decimals": 0
            }))),
            entity.options
        );
        assert_eq!(
            Some(ha_attributes(json!({ "state": "UNAVAILABLE" }))),
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
                "min": -5, "max": 5, "step": 0.5, "decimals": 1, "unit": "dB"
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
    fn convert_number_entity_with_too_small_step_is_limited() {
        let mut attr = ha_attributes(json!({ "min": 0, "max": 1, "step": 0.001 }));

        let (entity, state) =
            convert_number_entity("number.fine".into(), "0.123".into(), &mut attr)
                .expect("valid entity");

        let options = entity.options.expect("options");
        assert_eq!(Some(&json!(0.01)), options.get("step"));
        assert_eq!(Some(&json!(2)), options.get("decimals"));
        assert!(state.range.is_step_limited());
        assert_eq!(0.001, state.range.ha_step);
        // value is not rounded
        assert_eq!(
            Some(ha_attributes(json!({ "state": "ON", "value": 0.123 }))),
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
    #[case(json!({ "initial": 0, "step": 1 }), -1_000_000, 1_000_000)]
    #[case(json!({ "initial": 0, "step": 1, "minimum": null, "maximum": null }), -1_000_000, 1_000_000)]
    #[case(json!({ "initial": 0, "step": 1, "minimum": 0 }), 0, 1_000_000)]
    #[case(json!({ "initial": 0, "step": 1, "maximum": 20 }), -1_000_000, 20)]
    fn convert_unbounded_counter_uses_fallback_range(
        #[case] attr: Value,
        #[case] min: i64,
        #[case] max: i64,
    ) {
        let mut attr = ha_attributes(attr);

        let (entity, _) = convert_number_entity("counter.test".into(), "3".into(), &mut attr)
            .expect("valid entity");

        let options = entity.options.expect("options");
        assert_eq!(Some(&json!(min)), options.get("min"));
        assert_eq!(Some(&json!(max)), options.get("max"));
    }

    #[test]
    fn counter_ignores_min_max_attributes_of_number_entities() {
        let attr = ha_attributes(json!({ "min": 5, "max": 6, "step": 1 }));

        let info = parse_number_entity("counter.test", "3", Some(&attr));

        assert_eq!(-COUNTER_UNBOUNDED_LIMIT, info.range.min);
        assert_eq!(COUNTER_UNBOUNDED_LIMIT, info.range.max);
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
    #[case(&[0.001], 2)]
    #[case(&[1.0, 0.5], 1)]
    #[case(&[0.5, -10.25], 2)]
    #[case(&[3.0, -1_000_000.0], 0)]
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
            ha_attributes(json!({ "state": "UNAVAILABLE" })),
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
    #[case(range(0.0, 1.0, 0.001), 0.123, 0.12)]
    // clamping
    #[case(range(0.0, 100.0, 1.0), 101.0, 100.0)]
    #[case(range(0.0, 100.0, 1.0), -1.0, 0.0)]
    #[case(range(0.0, 10.0, 3.0), 10.0, 9.0)]
    #[case(range(0.0, 10.0, 3.0), 11.0, 10.0)]
    #[case(range(-10.0, 10.0, 1.0), -20.0, -10.0)]
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
    #[case(range(-COUNTER_UNBOUNDED_LIMIT, COUNTER_UNBOUNDED_LIMIT, 1.0), -3.0, -3.0)]
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
