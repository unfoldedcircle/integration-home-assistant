// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Switch entity specific HA event logic.

use uc_api::{intg::EntityChange, EntityType};

use crate::client::event::convert_ha_onoff_state;
use crate::client::model::EventData;
use crate::errors::ServiceError;

pub(crate) fn switch_event_to_entity_change(data: EventData) -> Result<EntityChange, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(1);
    let state = convert_ha_onoff_state(&data.new_state.state)?;

    attributes.insert("state".to_string(), state);

    Ok(EntityChange {
        device_id: None,
        entity_type: EntityType::Switch,
        entity_id: data.entity_id,
        attributes,
    })
}
