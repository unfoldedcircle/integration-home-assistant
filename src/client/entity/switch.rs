// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Switch entity specific logic.

use serde_json::{Map, Value};
use std::collections::HashMap;
use uc_api::intg::AvailableIntgEntity;
use uc_api::{intg::EntityChange, EntityType};

use crate::client::event::convert_ha_onoff_state;
use crate::client::model::EventData;
use crate::errors::ServiceError;

pub(crate) fn map_switch_attributes(
    _entity_id: &str,
    state: &str,
    _ha_attr: Option<&mut Map<String, Value>>,
) -> Result<Map<String, Value>, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(1);
    let state = convert_ha_onoff_state(state)?;

    attributes.insert("state".into(), state);

    Ok(attributes)
}

pub(crate) fn switch_event_to_entity_change(
    mut data: EventData,
) -> Result<EntityChange, ServiceError> {
    let attributes = map_switch_attributes(
        &data.entity_id,
        &data.new_state.state,
        data.new_state.attributes.as_mut(),
    )?;

    Ok(EntityChange {
        device_id: None,
        entity_type: EntityType::Switch,
        entity_id: data.entity_id,
        attributes,
    })
}

pub(crate) fn convert_switch_entity(
    entity_id: String,
    state: String,
    ha_attr: &mut Map<String, Value>,
) -> Result<AvailableIntgEntity, ServiceError> {
    let friendly_name = ha_attr.get("friendly_name").and_then(|v| v.as_str());
    let name = HashMap::from([("en".into(), friendly_name.unwrap_or(&entity_id).into())]);
    let device_class = ha_attr.get("device_class").and_then(|v| v.as_str());
    let device_class = match device_class {
        Some("outlet") | Some("switch") => device_class.map(|v| v.into()),
        _ => None,
    };

    let attributes = Some(map_switch_attributes(&entity_id, &state, Some(ha_attr))?);

    Ok(AvailableIntgEntity {
        entity_id,
        device_id: None, // prepared device_id handling
        entity_type: EntityType::Switch,
        device_class,
        name,
        features: Some(vec!["toggle".into()]), // OnOff is a default feature
        area: None,
        options: None,
        attributes,
    })
}
