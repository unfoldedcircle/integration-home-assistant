// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for Home Assistant events.

use crate::client::messages::{
    AssistEvent, AvailableEntities, EntityEvent, SetAvailableEntities, SubscribedEntities,
};
use crate::client::model::{AssistPipelineEvent, ResponseType};
use crate::controller::handler::{SubscribeHaEventsMsg, UnsubscribeHaEventsMsg};
use crate::controller::{Controller, OperationModeState, SendWsMessage};
use crate::errors::ServiceError;
use crate::util::DeserializeMsgData;
use actix::Handler;
use log::{debug, error, info};
use uc_api::intg::ws::{AvailableEntitiesMsgData, DriverEvent};
use uc_api::intg::{
    AssistantError, AssistantErrorCode, AssistantEvent, AssistantSpeechResponse,
    AssistantSttResponse, AssistantTextResponse, EntityChange, SubscribeEvents,
};
use uc_api::ws::{EventCategory, WsMessage};

impl Handler<EntityEvent> for Controller {
    type Result = ();

    fn handle(&mut self, msg: EntityEvent, _ctx: &mut Self::Context) -> Self::Result {
        // TODO keep an entity subscription per remote session and filter out non-subscribed remotes?
        if let Ok(msg_data) = serde_json::to_value(msg.entity_change) {
            for session in self.sessions.keys() {
                self.send_r2_msg(
                    WsMessage::event("entity_change", EventCategory::Entity, msg_data.clone()),
                    session,
                );
            }
        }
    }
}

impl Handler<AssistEvent> for Controller {
    type Result = ();

    fn handle(&mut self, msg: AssistEvent, _ctx: &mut Self::Context) -> Self::Result {
        // convert and propagate event to remote
        let entity_id = msg.entity_id;
        let session_id = msg.session_id;
        let event = match msg.event {
            AssistPipelineEvent::RunStart { .. } => AssistantEvent::Ready {
                entity_id,
                session_id,
            },
            AssistPipelineEvent::RunEnd => AssistantEvent::Finished {
                entity_id,
                session_id,
            },
            AssistPipelineEvent::WakeWordStart
            | AssistPipelineEvent::WakeWordEnd
            | AssistPipelineEvent::SttVadStart
            | AssistPipelineEvent::SttVadEnd
            | AssistPipelineEvent::SttStart { .. } => {
                // not used
                return;
            }
            AssistPipelineEvent::SttEnd { data } => {
                if let Some(output) = data.stt_output {
                    AssistantEvent::SttResponse {
                        entity_id,
                        session_id,
                        data: AssistantSttResponse::new(output.text),
                    }
                } else {
                    return;
                }
            }
            AssistPipelineEvent::IntentStart { .. } => return,
            AssistPipelineEvent::IntentProgress => return,
            AssistPipelineEvent::IntentEnd { data } => {
                if let Some(output) = data.intent_output
                    && let Some(response) = output.response
                {
                    if let Some(text) = response
                        .speech
                        .as_ref()
                        .and_then(|v| v.as_object())
                        .and_then(|map| map.get("plain"))
                        .and_then(|v| v.as_object())
                        .and_then(|map| map.get("speech"))
                        .and_then(|v| v.as_str())
                    {
                        let success = matches!(
                            response.response_type,
                            ResponseType::ActionDone | ResponseType::QueryAnswer
                        );
                        AssistantEvent::TextResponse {
                            entity_id,
                            session_id,
                            data: AssistantTextResponse::new(success, text),
                        }
                    } else {
                        info!(
                            "[{}] Unsupported intent response {:?} without text or SSML format",
                            self.remote_id, response.response_type
                        );
                        return;
                    }
                } else {
                    return;
                }
            }
            AssistPipelineEvent::TtsStart { .. } => return,
            AssistPipelineEvent::TtsEnd { data } => {
                if let Some(output) = data.tts_output {
                    AssistantEvent::SpeechResponse {
                        entity_id,
                        session_id,
                        data: AssistantSpeechResponse::new(output.url, output.mime_type),
                    }
                } else {
                    return;
                }
            }
            AssistPipelineEvent::Error { data } => {
                let code = match data.code.as_str() {
                    "timeout" => AssistantErrorCode::Timeout, // found while testing
                    "wake-engine-missing"
                    | "wake-provider-missing"
                    | "wake-stream-failed"
                    | "wake-word-timeout"
                    | "stt-provider-missing"
                    | "intent-not-supported"
                    | "tts-not-supported" => AssistantErrorCode::ServiceUnavailable,
                    "stt-provider-unsupported-metadata" => AssistantErrorCode::InvalidAudio,
                    "stt-no-text-recognized" => AssistantErrorCode::NoTextRecognized,
                    "stt-stream-failed" | "intent-failed" | "tts-failed" => {
                        AssistantErrorCode::UnexpectedError
                    }
                    _ => AssistantErrorCode::UnexpectedError,
                };
                AssistantEvent::Error {
                    entity_id,
                    session_id,
                    data: AssistantError::new(code, data.message),
                }
            }
        };

        let msg_data = serde_json::to_value(event).expect("BUG Failed to serialize AssistantEvent");
        for session in self.sessions.keys() {
            self.send_r2_msg(
                WsMessage::event(
                    DriverEvent::AssistantEvent.as_ref(),
                    EventCategory::Entity,
                    msg_data.clone(),
                ),
                session,
            );
        }
    }
}

impl Handler<AvailableEntities> for Controller {
    type Result = ();

    fn handle(&mut self, msg: AvailableEntities, _ctx: &mut Self::Context) -> Self::Result {
        // TODO just a quick implementation. Implement request filter! (also caching?)
        for (ws_id, session) in self.sessions.iter_mut() {
            if session.standby {
                debug!("[{ws_id}] Remote is in standby, not handling available_entities from HASS");
                continue;
            }
            if let Some(id) = session.get_available_entities_id {
                let msg_data = AvailableEntitiesMsgData {
                    filter: None,
                    available_entities: msg.entities.clone(),
                };
                if let Ok(msg_data_json) = serde_json::to_value(msg_data) {
                    match session
                        .recipient
                        .try_send(SendWsMessage(WsMessage::response(
                            id,
                            "available_entities",
                            msg_data_json.clone(),
                        ))) {
                        Ok(_) => session.get_available_entities_id = None,
                        Err(e) => error!("[{ws_id}] Error sending available_entities: {e:?}"),
                    }
                }
            } else if let Some(id) = session.get_entity_states_id {
                let mut msg_data = Vec::with_capacity(msg.entities.len());
                for entity in &msg.entities {
                    msg_data.push(EntityChange {
                        device_id: entity.device_id.clone(),
                        entity_type: entity.entity_type,
                        entity_id: entity.entity_id.clone(),
                        attributes: entity.attributes.clone().unwrap_or_default(),
                    });
                }

                if let Ok(msg_data_json) = serde_json::to_value(msg_data) {
                    match session
                        .recipient
                        .try_send(SendWsMessage(WsMessage::response(
                            id,
                            "entity_states",
                            msg_data_json.clone(),
                        ))) {
                        Ok(_) => session.get_entity_states_id = None,
                        Err(e) => error!("[{ws_id}] Error sending entity_states: {e:?}"),
                    }
                }
            }
        }
    }
}

impl Handler<SetAvailableEntities> for Controller {
    type Result = ();

    fn handle(&mut self, msg: SetAvailableEntities, _ctx: &mut Self::Context) -> Self::Result {
        for (ws_id, session) in self.sessions.iter_mut() {
            if session.standby {
                debug!(
                    "[{ws_id}] Remote is in standby, not handling set_available_entities from HASS"
                );
                continue;
            }
            let entity_ids: Vec<&String> = msg.entities.iter().map(|x| &x.entity_id).collect();
            debug!("[{ws_id}] Received new available entities to send to remote: {entity_ids:?}");
            // Store the list for next call to get_available_entities
            self.susbcribed_entity_ids = Option::from(msg.entities.clone());
        }
    }
}

impl Handler<SubscribeHaEventsMsg> for Controller {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, msg: SubscribeHaEventsMsg, _ctx: &mut Self::Context) -> Self::Result {
        if !matches!(self.machine.state(), &OperationModeState::Running) {
            return Err(ServiceError::ServiceUnavailable("Setup required".into()));
        }

        if let Some(session) = self.sessions.get_mut(&msg.0.ws_id) {
            let subscribe: SubscribeEvents = msg.0.deserialize()?;
            session.subscribed_entities.extend(subscribe.entity_ids);
            debug!("Sending updated subscribed entities to client for events subscriptions");
            if let Some(ha_client) = &self.ha_client {
                ha_client.try_send(SubscribedEntities {
                    entity_ids: session.subscribed_entities.clone(),
                })?;
            }
            Ok(())
        } else {
            Err(ServiceError::NotConnected)
        }
    }
}

impl Handler<UnsubscribeHaEventsMsg> for Controller {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, msg: UnsubscribeHaEventsMsg, _ctx: &mut Self::Context) -> Self::Result {
        if !matches!(self.machine.state(), &OperationModeState::Running) {
            return Err(ServiceError::ServiceUnavailable("Setup required".into()));
        }
        if let Some(session) = self.sessions.get_mut(&msg.0.ws_id) {
            debug!("UnsubscribeHaEventsMsg: {:?}", msg);
            let unsubscribe: SubscribeEvents = msg.0.deserialize()?;
            for i in unsubscribe.entity_ids {
                session.subscribed_entities.remove(&i);
            }
            if let Some(ha_client) = &self.ha_client {
                ha_client.try_send(SubscribedEntities {
                    entity_ids: session.subscribed_entities.clone(),
                })?;
            }
            Ok(())
        } else {
            Err(ServiceError::NotConnected)
        }
    }
}
