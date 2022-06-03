// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Button entity specific logic.

use crate::errors::ServiceError;
use serde_json::{Map, Value};
use std::collections::HashMap;
use uc_api::intg::AvailableIntgEntity;
use uc_api::EntityType;

pub(crate) fn convert_button_entity(
    entity_id: String,
    _state: String,
    ha_attr: &mut Map<String, Value>,
) -> Result<AvailableIntgEntity, ServiceError> {
    let friendly_name = ha_attr.get("friendly_name").and_then(|v| v.as_str());
    let name = HashMap::from([("en".into(), friendly_name.unwrap_or(&entity_id).into())]);

    Ok(AvailableIntgEntity {
        entity_id,
        device_id: None, // TODO prepare device_id handling
        entity_type: EntityType::Button,
        device_class: None,
        name,
        features: None, // no optional features, default = "press"
        area: None,
        options: None,
        attributes: None,
    })
}
