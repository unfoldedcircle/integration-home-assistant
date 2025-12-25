// Copyright (c) 2025 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Select entity specific logic.

use serde_json::{Map, Value};
use std::collections::HashMap;
use uc_api::intg::AvailableIntgEntity;
use uc_api::{EntityType, SelectAttribute, intg::EntityChange};

use crate::client::model::EventData;
use crate::errors::ServiceError;

pub(crate) fn map_select_attributes(
    _entity_id: &str,
    state: &str,
    ha_attr: Option<&mut Map<String, Value>>,
) -> Result<Map<String, Value>, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(3);

    // UC State (ON, UNAVAILABLE, UNKNOWN)
    let uc_state = match state {
        "unavailable" | "unknown" => state.to_uppercase(),
        _ => {
            let mut valid = false;
            if let Some(ha_attr) = &ha_attr {
                if let Some(options) = ha_attr.get("options").and_then(|v| v.as_array()) {
                    if options.iter().any(|v| v.as_str() == Some(state)) {
                        valid = true;
                    }
                }
            }
            if valid {
                "ON".to_string()
            } else {
                "UNKNOWN".to_string()
            }
        }
    };

    attributes.insert(SelectAttribute::State.to_string(), uc_state.into());

    // UC current_option (HA state)
    if state != "unavailable" && state != "unknown" {
        attributes.insert(SelectAttribute::CurrentOption.to_string(), state.into());
    }

    if let Some(ha_attr) = ha_attr {
        if let Some(options) = ha_attr.get("options") {
            attributes.insert(SelectAttribute::Options.to_string(), options.clone());
        }
    }

    Ok(attributes)
}

pub(crate) fn select_event_to_entity_change(
    mut data: EventData,
) -> Result<EntityChange, ServiceError> {
    let attributes = map_select_attributes(
        &data.entity_id,
        &data.new_state.state,
        data.new_state.attributes.as_mut(),
    )?;

    Ok(EntityChange {
        device_id: None,
        entity_type: EntityType::Select,
        entity_id: data.entity_id,
        attributes,
    })
}

pub(crate) fn convert_select_entity(
    entity_id: String,
    state: String,
    ha_attr: &mut Map<String, Value>,
) -> Result<AvailableIntgEntity, ServiceError> {
    let friendly_name = ha_attr.get("friendly_name").and_then(|v| v.as_str());
    let name = HashMap::from([("en".into(), friendly_name.unwrap_or(&entity_id).into())]);

    let attributes = Some(map_select_attributes(&entity_id, &state, Some(ha_attr))?);

    Ok(AvailableIntgEntity {
        entity_id,
        device_id: None,
        entity_type: EntityType::Select,
        device_class: None,
        name,
        icon: None,
        features: None,
        area: None,
        options: None,
        attributes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uc_api::SelectAttribute;

    #[test]
    fn test_select_event_to_entity_change() {
        let event_json = json!({
            "entity_id": "select.sync_box_hdmi_input",
            "old_state": {
                "entity_id": "select.sync_box_hdmi_input",
                "state": "AV Receiver",
                "attributes": {
                    "options": [
                        "HDMI 1",
                        "AV Receiver",
                        "HDMI 3",
                        "HDMI 4"
                    ],
                    "friendly_name": "Sync Box HDMI Input"
                },
                "last_changed": "2025-12-19T12:36:56.308722+00:00",
                "last_reported": "2025-12-19T12:37:07.942380+00:00",
                "last_updated": "2025-12-19T12:36:56.308722+00:00",
                "context": {
                    "id": "01KCV9SYVM3BHV9N07ZGMB6S1T",
                    "parent_id": null,
                    "user_id": null
                }
            },
            "new_state": {
                "entity_id": "select.sync_box_hdmi_input",
                "state": "HDMI 1",
                "attributes": {
                    "options": [
                        "HDMI 1",
                        "AV Receiver",
                        "HDMI 3",
                        "HDMI 4"
                    ],
                    "friendly_name": "Sync Box HDMI Input"
                },
                "last_changed": "2025-12-25T11:10:21.484657+00:00",
                "last_reported": "2025-12-25T11:10:21.484657+00:00",
                "last_updated": "2025-12-25T11:10:21.484657+00:00",
                "context": {
                    "id": "01KDAK7QQ95B229JT7MN1KWGCJ",
                    "parent_id": null,
                    "user_id": "08f6dc9d675e49ce8454a647e8216e4d"
                }
            }
        });

        let event_data: EventData = serde_json::from_value(event_json).unwrap();
        let result = select_event_to_entity_change(event_data).unwrap();

        assert_eq!(result.entity_id, "select.sync_box_hdmi_input");
        assert_eq!(result.entity_type, EntityType::Select);

        let attributes = result.attributes;
        assert_eq!(
            attributes.get(SelectAttribute::State.as_ref()).unwrap(),
            &json!("ON")
        );
        assert_eq!(
            attributes
                .get(SelectAttribute::CurrentOption.as_ref())
                .unwrap(),
            &json!("HDMI 1")
        );
        assert_eq!(
            attributes.get(SelectAttribute::Options.as_ref()).unwrap(),
            &json!(["HDMI 1", "AV Receiver", "HDMI 3", "HDMI 4"])
        );
    }

    #[test]
    fn test_convert_select_entity() {
        let mut ha_attr = json!({
            "options": [
                "HDMI 1",
                "AV Receiver",
                "HDMI 3",
                "HDMI 4"
            ],
            "friendly_name": "Sync Box HDMI Input"
        })
        .as_object()
        .unwrap()
        .clone();

        let entity_id = "select.sync_box_hdmi_input".to_string();
        let state = "AV Receiver".to_string();

        let result = convert_select_entity(entity_id.clone(), state, &mut ha_attr).unwrap();

        assert_eq!(result.entity_id, entity_id);
        assert_eq!(result.entity_type, EntityType::Select);
        assert_eq!(result.name.get("en").unwrap(), "Sync Box HDMI Input");

        let attributes = result.attributes.unwrap();
        assert_eq!(
            attributes.get(SelectAttribute::State.as_ref()).unwrap(),
            &json!("ON")
        );
        assert_eq!(
            attributes
                .get(SelectAttribute::CurrentOption.as_ref())
                .unwrap(),
            &json!("AV Receiver")
        );
        assert_eq!(
            attributes.get(SelectAttribute::Options.as_ref()).unwrap(),
            &json!(["HDMI 1", "AV Receiver", "HDMI 3", "HDMI 4"])
        );
    }

    #[test]
    fn test_select_state_mapping() {
        let mut ha_attr = json!({
            "options": ["Option 1", "Option 2"]
        })
        .as_object()
        .unwrap()
        .clone();

        // Valid option -> ON
        let attr = map_select_attributes("test", "Option 1", Some(&mut ha_attr)).unwrap();
        assert_eq!(attr.get(SelectAttribute::State.as_ref()).unwrap(), "ON");

        // Invalid option -> UNKNOWN
        let attr = map_select_attributes("test", "Option 3", Some(&mut ha_attr)).unwrap();
        assert_eq!(
            attr.get(SelectAttribute::State.as_ref()).unwrap(),
            "UNKNOWN"
        );

        // unavailable -> UNAVAILABLE
        let attr = map_select_attributes("test", "unavailable", Some(&mut ha_attr)).unwrap();
        assert_eq!(
            attr.get(SelectAttribute::State.as_ref()).unwrap(),
            "UNAVAILABLE"
        );

        // unknown -> UNKNOWN
        let attr = map_select_attributes("test", "unknown", Some(&mut ha_attr)).unwrap();
        assert_eq!(
            attr.get(SelectAttribute::State.as_ref()).unwrap(),
            "UNKNOWN"
        );
    }
}
