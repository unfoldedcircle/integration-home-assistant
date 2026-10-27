// Copyright (c) 2026 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home Assistant entity registry requests.

use crate::client::HomeAssistantClient;
use crate::client::messages::CallListEntityDisplayPrecisions;
use crate::client::model::OpenRequest;
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

impl Handler<CallListEntityDisplayPrecisions> for HomeAssistantClient {
    type Result = ResponseActFuture<Self, Result<HashMap<String, u32>, ServiceError>>;

    fn handle(
        &mut self,
        _msg: CallListEntityDisplayPrecisions,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let msg_id = self.new_msg_id();
        let (tx, rx) = oneshot::channel();
        self.open_requests.insert(msg_id, OpenRequest::new(tx));

        if let Err(e) = self.send_json(
            json!({ "id": msg_id, "type": "config/entity_registry/list_for_display" }),
            ctx,
        ) {
            self.open_requests.remove(&msg_id);
            return_fut_err!(e);
        };

        Box::pin(
            async move {
                let request_timeout = Duration::from_secs(5);
                match tokio::time::timeout(request_timeout, rx).await {
                    Ok(Ok(r)) => Ok(r),
                    _ => Err(ServiceError::ServiceUnavailable(
                        "Timeout while waiting for entity registry display list".into(),
                    )),
                }
            }
            .into_actor(self)
            .map(move |result, act, _ctx| {
                act.open_requests.remove(&msg_id);
                match result {
                    Ok(response) if response.success => Ok(parse_display_precisions(&response.msg)),
                    Ok(_) => Err(ServiceError::ServiceUnavailable(
                        "Entity registry display list request failed".into(),
                    )),
                    Err(e) => Err(e),
                }
            }),
        )
    }
}

/// Retrieve the sensor display precisions from Home Assistant.
///
/// Best effort: the `config/entity_registry/list_for_display` request requires Home Assistant
/// 2023.3 or newer. Returns `None` if the request failed.
pub(super) async fn fetch_display_precisions(
    ha_client: &Addr<HomeAssistantClient>,
) -> Option<HashMap<String, u32>> {
    match ha_client.send(CallListEntityDisplayPrecisions).await {
        Ok(Ok(result)) => Some(result),
        Ok(Err(e)) => {
            warn!("Failed to retrieve Home Assistant display precisions: {e}");
            None
        }
        Err(e) => {
            warn!("Failed to send CallListEntityDisplayPrecisions: {e}");
            None
        }
    }
}

impl HomeAssistantClient {
    /// Refresh the cached sensor display precisions in the background, e.g. after connecting.
    pub(super) fn refresh_display_precisions(&self, ctx: &mut Context<Self>) {
        let ha_client = ctx.address();
        ctx.spawn(
            async move { fetch_display_precisions(&ha_client).await }
                .into_actor(self)
                .map(|precisions, act, _ctx| {
                    if let Some(precisions) = precisions {
                        debug!(
                            "[{}] Got {} sensor display precisions",
                            act.id,
                            precisions.len()
                        );
                        act.display_precisions = precisions;
                    }
                }),
        );
    }
}

/// Parse the display precisions from a `config/entity_registry/list_for_display` response.
///
/// The entity id is in the `ei` field, the display precision in the optional `dp` field.
fn parse_display_precisions(msg: &Value) -> HashMap<String, u32> {
    msg.pointer("/result/entities")
        .and_then(|v| v.as_array())
        .map(|entities| {
            entities
                .iter()
                .filter_map(|e| {
                    let entity_id = e.get("ei")?.as_str()?;
                    let precision = e.get("dp")?.as_u64()?;
                    Some((entity_id.to_string(), u32::try_from(precision).ok()?))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_display_precisions_from_response() {
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
                    { "ei": "sensor.invalid", "dp": "1" },
                    { "dp": 1 }
                ]
            }
        });

        assert_eq!(
            HashMap::from([("sensor.shellyswitch25_98f4abf2c812_energy".to_string(), 2)]),
            parse_display_precisions(&msg)
        );
    }

    #[test]
    fn parse_display_precisions_without_entities() {
        assert!(parse_display_precisions(&json!({ "result": {} })).is_empty());
    }
}
