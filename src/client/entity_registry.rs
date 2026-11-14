// Copyright (c) 2026 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home Assistant entity registry requests.

use crate::client::HomeAssistantClient;
use crate::client::messages::{
    CallGetTranslations, CallListEntityRegistryDisplay, EntityRegistryDisplay,
};
use crate::client::model::OpenRequest;
use crate::client::translations::{SensorTranslations, TranslationKey};
use crate::errors::ServiceError;
use crate::util::return_fut_err;
use actix::{
    ActorFutureExt, Addr, AsyncContext, Context, Handler, ResponseActFuture, WrapFuture, fut,
};
use log::{debug, warn};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::oneshot;

impl Handler<CallListEntityRegistryDisplay> for HomeAssistantClient {
    type Result = ResponseActFuture<Self, Result<EntityRegistryDisplay, ServiceError>>;

    fn handle(
        &mut self,
        _msg: CallListEntityRegistryDisplay,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        self.request(
            json!({ "type": "config/entity_registry/list_for_display" }),
            "entity registry display list",
            ctx,
            parse_entity_registry_display,
        )
    }
}

impl HomeAssistantClient {
    /// Send a request to Home Assistant and parse the response message.
    ///
    /// # Arguments
    ///
    /// * `request`: request message without the message `id`
    /// * `name`: name of the requested data for error messages
    /// * `parse`: converts the successful response message
    pub(super) fn request<T: 'static>(
        &mut self,
        mut request: Value,
        name: &'static str,
        ctx: &mut Context<Self>,
        parse: impl FnOnce(&Value) -> T + 'static,
    ) -> ResponseActFuture<Self, Result<T, ServiceError>> {
        let msg_id = self.new_msg_id();
        request["id"] = msg_id.into();
        let (tx, rx) = oneshot::channel();
        self.open_requests.insert(msg_id, OpenRequest::new(tx));

        if let Err(e) = self.send_json(request, ctx) {
            self.open_requests.remove(&msg_id);
            return_fut_err!(e);
        };

        Box::pin(
            async move {
                let request_timeout = Duration::from_secs(5);
                match tokio::time::timeout(request_timeout, rx).await {
                    Ok(Ok(r)) => Ok(r),
                    _ => Err(ServiceError::ServiceUnavailable(format!(
                        "Timeout while waiting for {name}"
                    ))),
                }
            }
            .into_actor(self)
            .map(move |result, act, _ctx| {
                act.open_requests.remove(&msg_id);
                match result {
                    Ok(response) if response.success => Ok(parse(&response.msg)),
                    Ok(_) => Err(ServiceError::ServiceUnavailable(format!(
                        "Request failed: {name}"
                    ))),
                    Err(e) => Err(e),
                }
            }),
        )
    }

    /// Refresh the cached sensor display information in the background, e.g. after connecting.
    pub(super) fn refresh_sensor_display(&self, ctx: &mut Context<Self>) {
        let ha_client = ctx.address();
        ctx.spawn(
            async move { fetch_sensor_display(&ha_client).await }
                .into_actor(self)
                .map(|display, act, _ctx| {
                    if let Some(display) = display {
                        act.set_sensor_display(display);
                    }
                }),
        );
    }

    /// Store the retrieved sensor display information.
    pub(super) fn set_sensor_display(&mut self, display: SensorDisplay) {
        debug!(
            "[{}] Got {} sensor display precisions, {} sensor state translations",
            self.id,
            display.precisions.len(),
            display.translations.len()
        );
        self.display_precisions = display.precisions;
        self.sensor_translations = display.translations;
    }
}

/// Display information of the HA sensor entities.
pub(super) struct SensorDisplay {
    /// Map: HA entity ID -> display precision
    pub precisions: HashMap<String, u32>,
    /// State translations in the language of the Remote
    pub translations: SensorTranslations,
}

/// Retrieve the sensor display precisions and state translations from Home Assistant.
///
/// Best effort: the `config/entity_registry/list_for_display` request requires Home Assistant
/// 2023.3 or newer. Returns `None` if the request failed. The state translations are empty if they
/// can't be retrieved.
pub(super) async fn fetch_sensor_display(
    ha_client: &Addr<HomeAssistantClient>,
) -> Option<SensorDisplay> {
    let registry = match ha_client.send(CallListEntityRegistryDisplay).await {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => {
            warn!("Failed to retrieve Home Assistant entity registry display list: {e}");
            return None;
        }
        Err(e) => {
            warn!("Failed to send CallListEntityRegistryDisplay: {e}");
            return None;
        }
    };

    // translations of the integrations: only of the integrations with translated sensor entities
    let mut integrations: Vec<String> = registry
        .translation_keys
        .values()
        .map(|key| key.platform.clone())
        .collect();
    integrations.sort();
    integrations.dedup();

    let mut texts = HashMap::new();
    if !integrations.is_empty() {
        texts.extend(fetch_translations(ha_client, "entity", integrations).await);
    }
    // translations of the sensor device classes
    texts.extend(fetch_translations(ha_client, "entity_component", vec!["sensor".into()]).await);

    Some(SensorDisplay {
        precisions: registry.precisions,
        translations: SensorTranslations::new(registry.translation_keys, texts),
    })
}

/// Retrieve the sensor state translations of a translation category. Best effort: returns an
/// empty map if the request failed.
async fn fetch_translations(
    ha_client: &Addr<HomeAssistantClient>,
    category: &'static str,
    integrations: Vec<String>,
) -> HashMap<String, String> {
    let request = CallGetTranslations {
        category,
        integrations,
    };
    match ha_client.send(request).await {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => {
            warn!("Failed to retrieve Home Assistant {category} translations: {e}");
            HashMap::new()
        }
        Err(e) => {
            warn!("Failed to send CallGetTranslations: {e}");
            HashMap::new()
        }
    }
}

/// Parse the sensor display information from a `config/entity_registry/list_for_display` response.
///
/// Fields of an entity: the entity id is in `ei`, the optional display precision in `dp`, the
/// integration in `pl` and the optional translation key in `tk`. The translation keys are only
/// returned for `sensor` entities.
fn parse_entity_registry_display(msg: &Value) -> EntityRegistryDisplay {
    let mut display = EntityRegistryDisplay::default();
    let entities = msg.pointer("/result/entities").and_then(|v| v.as_array());
    for entity in entities.into_iter().flatten() {
        let Some(entity_id) = entity.get("ei").and_then(|v| v.as_str()) else {
            continue;
        };
        if let Some(precision) = entity
            .get("dp")
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok())
        {
            display.precisions.insert(entity_id.to_string(), precision);
        }
        if entity_id.starts_with("sensor.")
            && let Some(platform) = entity.get("pl").and_then(|v| v.as_str())
            && let Some(key) = entity.get("tk").and_then(|v| v.as_str())
        {
            display.translation_keys.insert(
                entity_id.to_string(),
                TranslationKey {
                    platform: platform.to_string(),
                    key: key.to_string(),
                },
            );
        }
    }
    display
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_entity_registry_display_from_response() {
        let msg = json!({
            "id": 27,
            "type": "result",
            "success": true,
            "result": {
                "entity_categories": { "0": "config", "1": "diagnostic" },
                "entities": [
                    {
                        "ei": "sensor.shellyswitch25_98f4abf2c812_energy",
                        "pl": "shelly",
                        "lb": [],
                        "di": "0236159c669305051a33ad6605a3142f",
                        "hn": true,
                        "en": "Energie",
                        "dp": 2
                    },
                    { "ei": "light.kitchen", "pl": "hue", "lb": [] },
                    {
                        "ei": "sensor.rocky_vacuum_error",
                        "pl": "roborock",
                        "tk": "vacuum_error",
                        "ec": 1
                    },
                    { "ei": "select.rocky_mop_mode", "pl": "roborock", "tk": "mop_mode" },
                    { "ei": "sensor.without_platform", "tk": "status" },
                    { "ei": "sensor.invalid", "dp": "1" },
                    { "dp": 1 }
                ]
            }
        });

        let display = parse_entity_registry_display(&msg);

        assert_eq!(
            HashMap::from([("sensor.shellyswitch25_98f4abf2c812_energy".to_string(), 2)]),
            display.precisions
        );
        assert_eq!(
            HashMap::from([(
                "sensor.rocky_vacuum_error".to_string(),
                TranslationKey {
                    platform: "roborock".into(),
                    key: "vacuum_error".into()
                }
            )]),
            display.translation_keys
        );
    }

    #[test]
    fn parse_entity_registry_display_without_entities() {
        let display = parse_entity_registry_display(&json!({ "result": {} }));

        assert!(display.precisions.is_empty());
        assert!(display.translation_keys.is_empty());
    }
}
