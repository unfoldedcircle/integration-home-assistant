// Copyright (c) 2024 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Remote entity specific logic.

use crate::client::event::convert_ha_onoff_state;
use crate::client::model::EventData;
use crate::errors::ServiceError;
use serde_json::{Map, Value};
use std::collections::HashMap;
use uc_api::intg::{AvailableIntgEntity, EntityChange, IntgRemoteFeature};
use uc_api::EntityType;

pub(crate) fn remote_event_to_entity_change(
    mut data: EventData,
) -> Result<EntityChange, ServiceError> {
    let attributes = map_remote_attributes(
        &data.entity_id,
        &data.new_state.state,
        data.new_state.attributes.as_mut(),
    )?;

    Ok(EntityChange {
        device_id: None,
        entity_type: EntityType::Remote,
        entity_id: data.entity_id,
        attributes,
    })
}

pub(crate) fn convert_remote_entity(
    entity_id: String,
    state: String,
    ha_attr: &mut Map<String, Value>,
) -> Result<AvailableIntgEntity, ServiceError> {
    let friendly_name = ha_attr.get("friendly_name").and_then(|v| v.as_str());
    let name = HashMap::from([("en".into(), friendly_name.unwrap_or(&entity_id).into())]);
    let attributes = Some(map_remote_attributes(&entity_id, &state, Some(ha_attr))?);

    Ok(AvailableIntgEntity {
        entity_id,
        device_id: None, // prepared device_id handling
        entity_type: EntityType::Remote,
        device_class: None,
        name,
        // toggle, on and off seem to be fixed features in HA
        features: Some(vec![
            IntgRemoteFeature::SendCmd.to_string(),
            IntgRemoteFeature::OnOff.to_string(),
            IntgRemoteFeature::Toggle.to_string(),
        ]),
        area: None,
        // Available commands are not retrievable from HA :-(
        // Feature proposition: https://github.com/home-assistant/architecture/discussions/875
        options: None,
        attributes,
    })
}

fn map_remote_attributes(
    _entity_id: &str,
    state: &str,
    _ha_attr: Option<&mut Map<String, Value>>,
) -> Result<Map<String, Value>, ServiceError> {
    let mut attributes = serde_json::Map::with_capacity(1);
    let state = convert_ha_onoff_state(state)?;

    attributes.insert("state".into(), state);

    Ok(attributes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::model::Event;
    use serde_json::json;
    use uc_api::EntityType;

    #[test]
    fn convert_ha_entity() {
        let mut ha_entity = json!({
          "entity_id": "remote.office_tv",
          "state": "on",
          "attributes": {
            "activity_list": null,
            "current_activity": "com.google.android.apps.tv.dreamx",
            "friendly_name": "Office TV",
            "supported_features": 4
          },
          "last_changed": "2024-04-04T19:24:45.412493+00:00",
          "last_updated": "2024-04-04T19:34:47.438673+00:00",
          "context": {
            "id": "01HTN9PJCE9WJHGKC5ZMAGVHBB",
            "parent_id": null,
            "user_id": null
          }
        });
        let ha_entity = ha_entity.as_object_mut().unwrap();

        let entity_id = ha_entity
            .get("entity_id")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        let state = ha_entity
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        let attr = ha_entity
            .get_mut("attributes")
            .and_then(|v| v.as_object_mut())
            .unwrap();

        let result = convert_remote_entity(entity_id, state, attr);
        assert!(
            result.is_ok(),
            "Expected successful entity conversion but got: {:?}",
            result.unwrap_err()
        );
        let entity = result.unwrap();

        assert_eq!("remote.office_tv", entity.entity_id);
        assert_eq!(EntityType::Remote, entity.entity_type);
        assert!(entity.features.is_some(), "Expected entity features");
        assert!(entity.attributes.is_some(), "Expected entity attributes");
        let attr = entity.attributes.unwrap();
        assert_eq!(1, attr.len());
        assert_eq!(Some(&json!("ON")), attr.get("state"));
    }

    #[test]
    fn remote_event_on() {
        let event = json!({
          "event_type": "state_changed",
          "data": {
            "entity_id": "remote.office_tv",
            "old_state": {
              "entity_id": "remote.office_tv",
              "state": "off",
              "attributes": {
                "activity_list": null,
                "current_activity": "com.google.android.apps.tv.launcherx",
                "friendly_name": "Office TV",
                "supported_features": 4
              },
              "last_changed": "2024-04-04T15:27:32.086304+00:00",
              "last_updated": "2024-04-04T19:24:44.282594+00:00",
              "context": {
                "id": "01HTN944487A1J7QMGBWYWS7P8",
                "parent_id": null,
                "user_id": "08f6dc9d675e49ce8454a647e8216e4d"
              }
            },
            "new_state": {
              "entity_id": "remote.office_tv",
              "state": "on",
              "attributes": {
                "activity_list": null,
                "current_activity": "com.google.android.apps.tv.launcherx",
                "friendly_name": "Office TV",
                "supported_features": 4
              },
              "last_changed": "2024-04-04T19:24:45.412493+00:00",
              "last_updated": "2024-04-04T19:24:45.412493+00:00",
              "context": {
                "id": "01HTN944487A1J7QMGBWYWS7P8",
                "parent_id": null,
                "user_id": "08f6dc9d675e49ce8454a647e8216e4d"
              }
            }
          },
          "origin": "LOCAL",
          "time_fired": "2024-04-04T19:24:45.412493+00:00",
          "context": {
            "id": "01HTN944487A1J7QMGBWYWS7P8",
            "parent_id": null,
            "user_id": "08f6dc9d675e49ce8454a647e8216e4d"
          }
        });

        let event = serde_json::from_value::<Event>(event).unwrap();
        let result = remote_event_to_entity_change(event.data);
        assert!(
            result.is_ok(),
            "Expected successful event mapping but got: {:?}",
            result.unwrap_err()
        );
        let entity_change = result.unwrap();
        assert_eq!("remote.office_tv", entity_change.entity_id);
        assert_eq!(EntityType::Remote, entity_change.entity_type);
        assert_eq!(1, entity_change.attributes.len());
        assert_eq!(Some(&json!("ON")), entity_change.attributes.get("state"));
    }
}
