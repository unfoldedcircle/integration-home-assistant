// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Button entity specific logic.

use crate::errors::ServiceError;
use serde_json::{Map, Value};
use std::collections::HashMap;
use uc_api::EntityType;
use uc_api::intg::AvailableIntgEntity;

pub(crate) fn convert_button_entity(
    entity_id: String,
    state: String,
    ha_attr: &mut Map<String, Value>,
) -> Result<AvailableIntgEntity, ServiceError> {
    let friendly_name = ha_attr.get("friendly_name").and_then(|v| v.as_str());
    let name = HashMap::from([("en".into(), friendly_name.unwrap_or(&entity_id).into())]);

    let state = if state == "unavailable" {
        "UNAVAILABLE"
    } else {
        "AVAILABLE"
    };
    let mut attributes = Map::with_capacity(1);
    attributes.insert("state".into(), Value::String(state.into()));

    Ok(AvailableIntgEntity {
        entity_id,
        device_id: None, // prepared for device_id handling
        entity_type: EntityType::Button,
        device_class: None,
        name,
        icon: None,
        description: None,
        features: None, // no optional features, default = "press"
        area: None,
        options: None,
        attributes: Some(attributes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use serde_json::json;

    #[rstest]
    #[case("off", "AVAILABLE")]
    #[case("on", "AVAILABLE")]
    #[case("unknown", "AVAILABLE")]
    #[case("2026-10-01T15:27:32.412816+00:00", "AVAILABLE")]
    #[case("unavailable", "UNAVAILABLE")]
    fn convert_button_sets_state_attribute(#[case] ha_state: &str, #[case] expected: &str) {
        let mut attr = Map::new();
        attr.insert("friendly_name".into(), json!("Test Button"));

        let entity =
            convert_button_entity("script.test".into(), ha_state.into(), &mut attr).unwrap();

        assert_eq!(entity.entity_type, EntityType::Button);
        assert_eq!(
            entity
                .attributes
                .as_ref()
                .and_then(|a| a.get("state"))
                .and_then(|v| v.as_str()),
            Some(expected)
        );
    }
}
