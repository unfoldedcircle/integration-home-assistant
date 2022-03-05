// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Sensor entity specific HA event logic.

use crate::client::model::EventData;
use crate::errors::ServiceError;
use crate::server::{EntityChange, EntityType};

use log::info;
use serde_json::Value;

pub(crate) fn sensor_event_to_entity_change(data: EventData) -> Result<EntityChange, ServiceError> {
    if data.entity_id.is_empty() || data.new_state.state.is_empty() {
        return Err(ServiceError::BadRequest(format!(
            "Missing data in state_changed event: {:?}",
            data
        )));
    }

    let mut attributes = serde_json::Map::with_capacity(2);
    attributes.insert("value".to_string(), Value::String(data.new_state.state));

    if let Some(mut ha_attr) = data.new_state.attributes {
        if let Some(uom) = ha_attr.remove("unit_of_measurement") {
            attributes.insert("unit".to_string(), uom);
        }
        // TODO check and handle attributes.device_class? E.g. checking for supported sensors.
        // Currently supported: "battery" | "current" | "energy" | "humidity" | "power" | "temperature" | "voltage"
    }

    Ok(EntityChange {
        device_id: None, // TODO set device_id, even if we don't support multiple HA instances (yet)
        entity_type: EntityType::Sensor,
        entity_id: data.entity_id,
        attributes,
    })
}

pub(crate) fn binary_sensor_event_to_entity_change(
    data: EventData,
) -> Result<EntityChange, ServiceError> {
    info!(
        "TODO handle binary sensor change event for {}: {:?}",
        data.entity_id, data.new_state.attributes
    );

    Err(ServiceError::NotYetImplemented)
}
