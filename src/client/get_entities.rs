// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix actor handler implementation for the `GetStates` message

use crate::client::HomeAssistantClient;
use crate::client::messages::{CallListAssistPipelines, GetAvailableEntities};
use crate::errors::ServiceError;
use actix::{ActorFutureExt, AsyncContext, Handler, ResponseActFuture, WrapFuture};
use log::{debug, error, info};
use serde_json::json;
use std::collections::HashMap;
use uc_api::intg::AvailableIntgEntity;
use uc_api::{
    AudioConfiguration, EntityType, VoiceAssistantAttribute, VoiceAssistantEntityOptions,
    VoiceAssistantFeature, VoiceAssistantProfile,
};

impl Handler<GetAvailableEntities> for HomeAssistantClient {
    type Result = ResponseActFuture<Self, Result<(), ServiceError>>;

    fn handle(&mut self, msg: GetAvailableEntities, ctx: &mut Self::Context) -> Self::Result {
        debug!("[{}] GetAvailableEntities from {}", self.id, msg.remote_id);
        self.remote_id = msg.remote_id;

        let ha_client = ctx.address();
        Box::pin(
            async move {
                // Retrieve available assist pipelines and map them to UC voice-assistant entities
                // This is best effort, old HA setups might return an error
                match ha_client.send(CallListAssistPipelines::default()).await {
                    Ok(Ok(result)) => Some(result),
                    Ok(Err(e)) => {
                        error!("Failed to retrieve Home Assistant assist pipelines: {e}");
                        None
                    }
                    Err(e) => {
                        error!("Failed to send CallListAssistPipelines: {e}");
                        None
                    }
                }
            }
            .into_actor(self) // converts future to ActorFuture
            .map(move |result, act, ctx| {
                if let Some(result) = result {
                    info!("Got assist pipelines: {result:?}");
                    act.assist_pipelines = Some(result);
                }
                // Retrieve Home Assistant entities
                let id = act.new_msg_id();

                act.entity_states_id = Some(id);
                // Try to subscribe again to custom events if not already done when
                // GetAvailableEntities command is received from the remote
                act.send_uc_info_command(ctx);
                if act.uc_ha_component {
                    // Retrieve the states of available entities (including subscribed entities)
                    // Available entities are defined on HA component side and should include
                    // subscribed entities but sent anyway just in case some are missing
                    debug!(
                        "[{}] Get states from {} with unfoldedcircle/get_states",
                        act.id, act.remote_id
                    );
                    act.send_json(
                        json!(
                            {"id": id, "type": "unfoldedcircle/entities/states",
                            "data": {
                                "entity_ids": act.subscribed_entities,
                                "client_id": act.remote_id
                            }}
                        ),
                        ctx,
                    )
                } else {
                    debug!("[{}] Get standard states from {} ", act.id, act.remote_id);
                    act.send_json(
                        json!(
                            {"id": id, "type": "get_states"}
                        ),
                        ctx,
                    )
                }
            }),
        )
    }
}

impl HomeAssistantClient {
    pub(super) fn get_voice_assistant_entities(&self) -> Vec<AvailableIntgEntity> {
        if let Some(assist) = &self.assist_pipelines
            && !assist.pipelines.is_empty()
        {
            let name = HashMap::from([
                ("en".into(), "Voice assistant (Assist)".into()),
                ("de".into(), "Sprachassistent (Assist)".into()),
                ("es".into(), "Asistente de voz (Assist)".into()),
                ("fr".into(), "Assistant vocal (Assist)".into()),
                ("it".into(), "Assistenti vocali (Assist)".into()),
                ("nl".into(), "Spraakassistent (Assist)".into()),
                ("no".into(), "Språkassistent (Assist)".into()),
                ("pt".into(), "Assistente de voz (Assist)".into()),
                ("sv".into(), "Språkassistent (Assist)".into()),
            ]);

            let pref_id = assist.preferred_pipeline.as_deref().unwrap_or_default();
            let pref_pipe = assist.pipelines.iter().find(|p| p.id == pref_id);

            let mut features = vec![
                VoiceAssistantFeature::Transcription.to_string(),
                VoiceAssistantFeature::ResponseText.to_string(),
            ];

            if let Some(pref_pipe) = pref_pipe
                && pref_pipe.stt_engine.is_some()
            {
                features.push(VoiceAssistantFeature::ResponseSpeech.to_string());
            }

            let profiles = assist
                .pipelines
                .iter()
                .map(|p| {
                    // afaik, transcription & text response are always active
                    let mut prof_feat = vec![
                        VoiceAssistantFeature::Transcription,
                        VoiceAssistantFeature::ResponseText,
                    ];
                    // speech response can be configured in a pipeline and might not be active for all.
                    if p.stt_engine.is_some() {
                        prof_feat.push(VoiceAssistantFeature::ResponseSpeech);
                    }
                    VoiceAssistantProfile {
                        id: p.id.clone(),
                        name: p.name.clone(),
                        language: Some(p.language.clone()),
                        features: Some(prof_feat),
                    }
                })
                .collect();

            let options = VoiceAssistantEntityOptions {
                audio_cfg: Some(AudioConfiguration::default()),
                profiles: Some(profiles),
                preferred_profile: pref_pipe.map(|p| p.id.clone()),
            };

            let mut attributes = serde_json::Map::with_capacity(1);
            attributes.insert(VoiceAssistantAttribute::State.to_string(), "ON".into());

            let entity = AvailableIntgEntity {
                entity_id: "assist".to_string(),
                device_id: None,
                entity_type: EntityType::VoiceAssistant,
                device_class: None,
                name,
                icon: Some("uc:microphone".to_string()),
                features: Some(features),
                area: None,
                options: None,
                attributes: Some(attributes),
            }
            .with_options(options)
            .expect("BUG invalid voice assistant entity options");

            return vec![entity];
        }

        vec![]
    }
}
