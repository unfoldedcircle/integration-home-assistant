// Copyright (c) 2026 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Number entity specific HA service call logic for `number`, `input_number` and `counter`
//! entities.
//!
//! - `set_value`: the value is rounded to the step size and clamped to the current HA value range.
//!   HA rejects out-of-range values, and `counter.set_value` requires a multiple of the step size.
//! - `increment` / `decrement`: `input_number` and `counter` provide native services using the
//!   configured step size. These are used if no custom step size is requested. Otherwise, and for
//!   `number` entities without such services, the new value is calculated from the current value.

use crate::client::entity::{NumberState, number_to_json};
use crate::client::{cmd_from_str, get_required_params};
use crate::errors::ServiceError;
use log::debug;
use serde_json::{Value, json};
use std::time::Instant;
use uc_api::NumberCommand;
use uc_api::intg::EntityCommand;

/// HA service call of a number entity command.
#[derive(Debug, PartialEq)]
pub(crate) struct NumberServiceCall {
    pub service: String,
    pub service_data: Option<Value>,
    /// Expected new value after the service call, if it can be determined.
    pub expected_value: Option<f64>,
}

/// Convert a number entity command to a HA service call.
///
/// # Arguments
///
/// * `msg`: number entity command.
/// * `domain`: HA entity domain: `number`, `input_number` or `counter`.
/// * `state`: cached number entity state, if available.
/// * `now`: current timestamp to determine the current value.
pub(crate) fn handle_number(
    msg: &EntityCommand,
    domain: &str,
    state: Option<&NumberState>,
    now: Instant,
) -> Result<NumberServiceCall, ServiceError> {
    let cmd: NumberCommand = cmd_from_str(&msg.cmd_id)?;
    let counter = domain == "counter";

    match cmd {
        NumberCommand::SetValue => {
            let params = get_required_params(msg)?;
            let value = params
                .get("value")
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite())
                .ok_or_else(|| {
                    ServiceError::BadRequest("Invalid or missing params.value attribute".into())
                })?;
            if params.contains_key("duration") {
                debug!(
                    "Ignoring duration parameter of {}: not supported by Home Assistant",
                    msg.entity_id
                );
            }

            let value = match state {
                Some(state) => state.range.normalize(value, counter),
                None if counter => value.round(),
                None => value,
            };
            Ok(set_value(value, counter))
        }
        NumberCommand::Increment | NumberCommand::Decrement => {
            let step = match msg.params.as_ref().and_then(|p| p.get("step")) {
                None => None,
                Some(step) => Some(
                    step.as_f64()
                        .filter(|v| v.is_finite() && *v > 0.0)
                        .ok_or_else(|| {
                            ServiceError::BadRequest(
                                "Invalid params.step attribute: must be a positive number".into(),
                            )
                        })?,
                ),
            };
            let sign = if cmd == NumberCommand::Increment {
                1.0
            } else {
                -1.0
            };

            // Native increment & decrement services use the configured step size of the entity
            let native_service = matches!(domain, "input_number" | "counter");
            let use_native_service = native_service
                && match (step.or(state.map(|s| s.range.step)), state) {
                    (None, _) => true,
                    (Some(step), Some(state)) => is_equal(step, state.range.ha_step),
                    (Some(_), None) => false,
                };

            if use_native_service {
                let expected_value = state.and_then(|state| {
                    state
                        .current_value(now)
                        .map(|value| state.range.clamp(value + sign * state.range.ha_step))
                });
                return Ok(NumberServiceCall {
                    service: cmd.to_string(),
                    service_data: None,
                    expected_value,
                });
            }

            let unknown_value = || {
                ServiceError::ServiceUnavailable(format!(
                    "Current value of {} is not known",
                    msg.entity_id
                ))
            };
            let state = state.ok_or_else(unknown_value)?;
            let value = state.current_value(now).ok_or_else(unknown_value)?;
            let step = step.unwrap_or(state.range.step);

            Ok(set_value(
                state.range.normalize(value + sign * step, counter),
                counter,
            ))
        }
    }
}

fn set_value(value: f64, counter: bool) -> NumberServiceCall {
    let value_json = if counter {
        // counter.set_value requires an integer
        json!(value as i64)
    } else {
        number_to_json(value)
    };

    NumberServiceCall {
        service: "set_value".into(),
        service_data: Some(json!({ "value": value_json })),
        expected_value: Some(value),
    }
}

fn is_equal(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::entity::{MIN_STEP, NumberRange, OPTIMISTIC_VALUE_TIMEOUT, decimals_for};
    use rstest::rstest;
    use std::time::Duration;
    use uc_api::EntityType;

    fn new_entity_command(cmd_id: impl Into<String>, params: Value) -> EntityCommand {
        EntityCommand {
            device_id: None,
            entity_type: EntityType::Number,
            entity_id: "test".into(),
            cmd_id: cmd_id.into(),
            params: params.as_object().cloned(),
        }
    }

    fn number_state(min: f64, max: f64, step: f64, value: Option<f64>) -> NumberState {
        unbounded_number_state(Some(min), Some(max), step, value)
    }

    fn unbounded_number_state(
        min: Option<f64>,
        max: Option<f64>,
        step: f64,
        value: Option<f64>,
    ) -> NumberState {
        let uc_step = step.max(MIN_STEP);
        NumberState {
            range: NumberRange {
                min,
                max,
                step: uc_step,
                ha_step: step,
                decimals: decimals_for(&[uc_step, min.unwrap_or_default()]),
            },
            value,
            optimistic: None,
        }
    }

    fn set_value_call(value: Value, expected_value: f64) -> NumberServiceCall {
        NumberServiceCall {
            service: "set_value".into(),
            service_data: Some(json!({ "value": value })),
            expected_value: Some(expected_value),
        }
    }

    #[test]
    fn invalid_command_returns_bad_request() {
        let cmd = new_entity_command("foobar", Value::Null);

        let result = handle_number(&cmd, "number", None, Instant::now());

        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "Invalid cmd_id must return BadRequest, but got: {result:?}"
        );
    }

    #[rstest]
    #[case(Value::Null)]
    #[case(json!({}))]
    #[case(json!({ "value": null }))]
    #[case(json!({ "value": "42" }))]
    #[case(json!({ "value": true }))]
    #[case(json!({ "duration": 1000 }))]
    fn set_value_with_invalid_value_returns_bad_request(#[case] params: Value) {
        let cmd = new_entity_command("set_value", params);
        let state = number_state(0.0, 100.0, 1.0, Some(50.0));

        let result = handle_number(&cmd, "number", Some(&state), Instant::now());

        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "Invalid set_value parameters must return BadRequest, but got: {result:?}"
        );
    }

    #[rstest]
    #[case("number", json!(42), json!(42), 42.0)]
    #[case("number", json!(42.0), json!(42), 42.0)]
    #[case("number", json!(42.4), json!(42), 42.0)]
    #[case("number", json!(101), json!(100), 100.0)]
    #[case("number", json!(-1), json!(0), 0.0)]
    #[case("input_number", json!(42.6), json!(43), 43.0)]
    #[case("counter", json!(42.6), json!(43), 43.0)]
    #[case("counter", json!(101), json!(100), 100.0)]
    fn set_value_is_rounded_and_clamped(
        #[case] domain: &str,
        #[case] value: Value,
        #[case] expected_json: Value,
        #[case] expected_value: f64,
    ) {
        let cmd = new_entity_command("set_value", json!({ "value": value }));
        let state = number_state(0.0, 100.0, 1.0, Some(50.0));

        let result = handle_number(&cmd, domain, Some(&state), Instant::now());

        assert_eq!(Ok(set_value_call(expected_json, expected_value)), result);
    }

    #[test]
    fn set_value_with_fractional_step() {
        let cmd = new_entity_command("set_value", json!({ "value": 1.3 }));
        let state = number_state(-5.0, 5.0, 0.5, Some(0.0));

        let result = handle_number(&cmd, "input_number", Some(&state), Instant::now());

        assert_eq!(Ok(set_value_call(json!(1.5), 1.5)), result);
    }

    #[test]
    fn set_value_with_duration_ignores_duration() {
        let cmd = new_entity_command("set_value", json!({ "value": 80, "duration": 2000 }));
        let state = number_state(0.0, 100.0, 1.0, Some(50.0));

        let result = handle_number(&cmd, "number", Some(&state), Instant::now());

        assert_eq!(Ok(set_value_call(json!(80), 80.0)), result);
    }

    #[rstest]
    #[case(json!(5.0), json!(6), 6.0)]
    #[case(json!(4.9), json!(4), 4.0)]
    #[case(json!(0), json!(2), 2.0)]
    #[case(json!(10), json!(8), 8.0)]
    fn counter_set_value_uses_multiple_of_step(
        #[case] value: Value,
        #[case] expected_json: Value,
        #[case] expected_value: f64,
    ) {
        let cmd = new_entity_command("set_value", json!({ "value": value }));
        let state = number_state(1.0, 9.0, 2.0, Some(4.0));

        let result = handle_number(&cmd, "counter", Some(&state), Instant::now());

        assert_eq!(Ok(set_value_call(expected_json, expected_value)), result);
    }

    #[rstest]
    #[case("number", json!(42.25), json!(42.25), 42.25)]
    #[case("input_number", json!(-3), json!(-3), -3.0)]
    #[case("counter", json!(4.6), json!(5), 5.0)]
    fn set_value_without_cached_state_passes_value(
        #[case] domain: &str,
        #[case] value: Value,
        #[case] expected_json: Value,
        #[case] expected_value: f64,
    ) {
        let cmd = new_entity_command("set_value", json!({ "value": value }));

        let result = handle_number(&cmd, domain, None, Instant::now());

        assert_eq!(Ok(set_value_call(expected_json, expected_value)), result);
    }

    #[rstest]
    #[case("increment", Some(51.0))]
    #[case("decrement", Some(49.0))]
    fn native_increment_decrement_without_step(
        #[case] cmd_id: &str,
        #[case] expected_value: Option<f64>,
        #[values("input_number", "counter")] domain: &str,
    ) {
        let cmd = new_entity_command(cmd_id, Value::Null);
        let state = number_state(0.0, 100.0, 1.0, Some(50.0));

        let result = handle_number(&cmd, domain, Some(&state), Instant::now());

        assert_eq!(
            Ok(NumberServiceCall {
                service: cmd_id.into(),
                service_data: None,
                expected_value,
            }),
            result
        );
    }

    #[rstest]
    #[case("increment")]
    #[case("decrement")]
    fn native_increment_decrement_without_cached_state(
        #[case] cmd_id: &str,
        #[values("input_number", "counter")] domain: &str,
    ) {
        let cmd = new_entity_command(cmd_id, Value::Null);

        let result = handle_number(&cmd, domain, None, Instant::now());

        assert_eq!(
            Ok(NumberServiceCall {
                service: cmd_id.into(),
                service_data: None,
                expected_value: None,
            }),
            result
        );
    }

    #[rstest]
    #[case("increment", 100.0)]
    #[case("decrement", 0.0)]
    fn native_increment_decrement_expected_value_is_clamped(
        #[case] cmd_id: &str,
        #[case] value: f64,
    ) {
        let cmd = new_entity_command(cmd_id, Value::Null);
        let state = number_state(0.0, 100.0, 1.0, Some(value));

        let result = handle_number(&cmd, "input_number", Some(&state), Instant::now());

        assert_eq!(Some(value), result.expect("valid call").expected_value);
    }

    #[test]
    fn native_increment_with_entity_step_parameter() {
        let cmd = new_entity_command("increment", json!({ "step": 0.5 }));
        let state = number_state(0.0, 10.0, 0.5, Some(2.0));

        let result = handle_number(&cmd, "input_number", Some(&state), Instant::now());

        assert_eq!(
            Ok(NumberServiceCall {
                service: "increment".into(),
                service_data: None,
                expected_value: Some(2.5),
            }),
            result
        );
    }

    #[rstest]
    #[case("input_number", "increment", json!(55), 55.0)]
    #[case("input_number", "decrement", json!(45), 45.0)]
    #[case("counter", "increment", json!(55), 55.0)]
    #[case("counter", "decrement", json!(45), 45.0)]
    fn increment_decrement_with_custom_step_uses_set_value(
        #[case] domain: &str,
        #[case] cmd_id: &str,
        #[case] expected_json: Value,
        #[case] expected_value: f64,
    ) {
        let cmd = new_entity_command(cmd_id, json!({ "step": 5 }));
        let state = number_state(0.0, 100.0, 1.0, Some(50.0));

        let result = handle_number(&cmd, domain, Some(&state), Instant::now());

        assert_eq!(Ok(set_value_call(expected_json, expected_value)), result);
    }

    #[test]
    fn increment_with_custom_step_without_cached_state_is_unavailable() {
        let cmd = new_entity_command("increment", json!({ "step": 5 }));

        let result = handle_number(&cmd, "input_number", None, Instant::now());

        assert!(
            matches!(result, Err(ServiceError::ServiceUnavailable(_))),
            "Unknown current value must return ServiceUnavailable, but got: {result:?}"
        );
    }

    #[test]
    fn input_number_with_limited_step_uses_set_value() {
        // HA step 0.0000001 is limited to 0.000001: native increment would use the HA step size
        let cmd = new_entity_command("increment", Value::Null);
        let state = number_state(0.0, 1.0, 0.0000001, Some(0.5));

        let result = handle_number(&cmd, "input_number", Some(&state), Instant::now());

        assert_eq!(Ok(set_value_call(json!(0.500001), 0.500001)), result);
    }

    #[rstest]
    #[case("counter", json!({ "value": -1234 }), json!(-1234), -1234.0)]
    #[case("counter", json!({ "value": 1_000_001.4 }), json!(1_000_001), 1_000_001.0)]
    #[case("number", json!({ "value": -1234.3 }), json!(-1234), -1234.0)]
    fn set_value_without_limits(
        #[case] domain: &str,
        #[case] params: Value,
        #[case] expected_json: Value,
        #[case] expected_value: f64,
    ) {
        let cmd = new_entity_command("set_value", params);
        let state = unbounded_number_state(None, None, 1.0, Some(0.0));

        let result = handle_number(&cmd, domain, Some(&state), Instant::now());

        assert_eq!(Ok(set_value_call(expected_json, expected_value)), result);
    }

    #[rstest]
    #[case("increment", Some(0.0), json!(0), 0.0)]
    #[case("increment", None, json!(2), 2.0)]
    #[case("decrement", Some(0.0), json!(-8), -8.0)]
    #[case("decrement", None, json!(-8), -8.0)]
    fn counter_increment_decrement_with_custom_step_and_one_limit(
        #[case] cmd_id: &str,
        #[case] max: Option<f64>,
        #[case] expected_json: Value,
        #[case] expected_value: f64,
    ) {
        let cmd = new_entity_command(cmd_id, json!({ "step": 5 }));
        let state = unbounded_number_state(None, max, 1.0, Some(-3.0));

        let result = handle_number(&cmd, "counter", Some(&state), Instant::now());

        assert_eq!(Ok(set_value_call(expected_json, expected_value)), result);
    }

    #[rstest]
    #[case("increment", Some(1_000_001.0))]
    #[case("decrement", Some(999_999.0))]
    fn native_counter_increment_decrement_without_limits(
        #[case] cmd_id: &str,
        #[case] expected_value: Option<f64>,
    ) {
        let cmd = new_entity_command(cmd_id, Value::Null);
        let state = unbounded_number_state(None, None, 1.0, Some(1_000_000.0));

        let result = handle_number(&cmd, "counter", Some(&state), Instant::now());

        assert_eq!(
            Ok(NumberServiceCall {
                service: cmd_id.into(),
                service_data: None,
                expected_value,
            }),
            result
        );
    }

    #[rstest]
    #[case("increment", Value::Null, json!(51), 51.0)]
    #[case("decrement", Value::Null, json!(49), 49.0)]
    #[case("increment", json!({ "step": 10 }), json!(60), 60.0)]
    #[case("decrement", json!({ "step": 10 }), json!(40), 40.0)]
    #[case("increment", json!({ "step": 0.4 }), json!(50), 50.0)]
    #[case("increment", json!({ "step": 0.6 }), json!(51), 51.0)]
    #[case("increment", json!({ "step": 100 }), json!(100), 100.0)]
    #[case("decrement", json!({ "step": 100 }), json!(0), 0.0)]
    fn number_increment_decrement_uses_set_value(
        #[case] cmd_id: &str,
        #[case] params: Value,
        #[case] expected_json: Value,
        #[case] expected_value: f64,
    ) {
        let cmd = new_entity_command(cmd_id, params);
        let state = number_state(0.0, 100.0, 1.0, Some(50.0));

        let result = handle_number(&cmd, "number", Some(&state), Instant::now());

        assert_eq!(Ok(set_value_call(expected_json, expected_value)), result);
    }

    #[test]
    fn number_increment_with_fractional_step() {
        let cmd = new_entity_command("increment", Value::Null);
        let state = number_state(-10.0, 10.0, 0.5, Some(2.0));

        let result = handle_number(&cmd, "number", Some(&state), Instant::now());

        assert_eq!(Ok(set_value_call(json!(2.5), 2.5)), result);
    }

    #[rstest]
    #[case("increment")]
    #[case("decrement")]
    fn number_increment_decrement_with_unknown_value_is_unavailable(#[case] cmd_id: &str) {
        let cmd = new_entity_command(cmd_id, Value::Null);
        let state = number_state(0.0, 100.0, 1.0, None);

        for state in [None, Some(&state)] {
            let result = handle_number(&cmd, "number", state, Instant::now());

            assert!(
                matches!(result, Err(ServiceError::ServiceUnavailable(_))),
                "Unknown current value must return ServiceUnavailable, but got: {result:?}"
            );
        }
    }

    #[rstest]
    #[case(json!({ "step": 0 }))]
    #[case(json!({ "step": -1 }))]
    #[case(json!({ "step": "1" }))]
    #[case(json!({ "step": null }))]
    fn increment_decrement_with_invalid_step_returns_bad_request(
        #[case] params: Value,
        #[values("increment", "decrement")] cmd_id: &str,
        #[values("number", "input_number", "counter")] domain: &str,
    ) {
        let cmd = new_entity_command(cmd_id, params);
        let state = number_state(0.0, 100.0, 1.0, Some(50.0));

        let result = handle_number(&cmd, domain, Some(&state), Instant::now());

        assert!(
            matches!(result, Err(ServiceError::BadRequest(_))),
            "Invalid step must return BadRequest, but got: {result:?}"
        );
    }

    #[test]
    fn repeated_increments_use_optimistic_value() {
        let cmd = new_entity_command("increment", Value::Null);
        let mut state = number_state(0.0, 100.0, 1.0, Some(50.0));
        let now = Instant::now();

        for expected in [51.0, 52.0, 53.0] {
            let call = handle_number(&cmd, "number", Some(&state), now).expect("valid call");
            assert_eq!(Some(expected), call.expected_value);
            // as done by the service handler after sending the service call
            state.optimistic = call.expected_value.map(|v| (v, now));
        }
    }

    #[test]
    fn increment_after_optimistic_value_timeout_uses_ha_value() {
        let cmd = new_entity_command("increment", Value::Null);
        let mut state = number_state(0.0, 100.0, 1.0, Some(50.0));
        let now = Instant::now();
        state.optimistic = Some((60.0, now));

        let call = handle_number(
            &cmd,
            "number",
            Some(&state),
            now + OPTIMISTIC_VALUE_TIMEOUT + Duration::from_millis(1),
        )
        .expect("valid call");

        assert_eq!(Some(51.0), call.expected_value);
    }

    #[test]
    fn native_increment_expected_value_uses_optimistic_value() {
        let cmd = new_entity_command("increment", Value::Null);
        let mut state = number_state(0.0, 100.0, 1.0, Some(50.0));
        let now = Instant::now();
        state.optimistic = Some((52.0, now));

        let call = handle_number(&cmd, "counter", Some(&state), now).expect("valid call");

        assert_eq!(Some(53.0), call.expected_value);
    }
}
