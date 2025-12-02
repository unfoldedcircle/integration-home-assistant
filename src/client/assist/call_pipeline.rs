// Copyright (c) 2025 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home Assistant WebSocket assist pipeline call handler.

use crate::client::HomeAssistantClient;
use crate::client::messages::{CallListAssistPipelines, CallRunAssistPipeline};
use crate::client::model::{
    AssistPipelineMsg, AssistPipelineRequest, AssistSession, EndStage, GetPipelinesResult,
    OpenRequest, ResponseMsg, RunAssistPipelineBuilder,
};
use crate::errors::ServiceError;
use crate::errors::ServiceError::InternalServerError;
use crate::util::return_fut_err;
use actix::{ActorFutureExt, Handler, ResponseActFuture, WrapFuture, fut};
use log::{error, info, warn};
use std::time::Duration;
use tokio::sync::oneshot;

impl Handler<CallRunAssistPipeline> for HomeAssistantClient {
    type Result = ResponseActFuture<Self, Result<(), ServiceError>>;

    fn handle(&mut self, msg: CallRunAssistPipeline, ctx: &mut Self::Context) -> Self::Result {
        self.remove_expired_assist_sessions();

        let msg_id = self.new_msg_id();
        info!(
            "[{}] Starting assist session with request_id {msg_id}, session id {}",
            self.id, msg.session_id
        );
        self.assist_sessions.insert(
            msg_id,
            AssistSession::new(msg_id, msg.entity_id, msg.session_id),
        );
        let (tx, rx) = oneshot::channel();
        self.open_requests.insert(msg_id, OpenRequest::new(tx));

        let run = match RunAssistPipelineBuilder::default()
            .id(msg_id)
            .input(serde_json::json!({ "sample_rate": msg.sample_rate }))
            .timeout(msg.timeout)
            .end_stage(if msg.speech_response {
                EndStage::Tts
            } else {
                EndStage::Intent
            })
            .pipeline(msg.pipeline_id)
            .build()
            .map_err(|e| InternalServerError(format!("Error building pipeline run request: {e}")))
        {
            Ok(run) => run,
            Err(e) => {
                return_fut_err!(e);
            }
        };
        let assist_pipeline_req = AssistPipelineRequest::Run(run);

        if let Err(e) = self.send_json(
            serde_json::to_value(assist_pipeline_req).expect("BUG serializing"),
            ctx,
        ) {
            self.assist_sessions.remove(&msg_id);
            self.open_requests.remove(&msg_id);
            return_fut_err!(e);
        };

        Box::pin(
            async move {
                let request_timeout = Duration::from_secs(5);
                match tokio::time::timeout(request_timeout, rx).await {
                    Ok(Ok(r)) => Ok(r),
                    _ => Err(ServiceError::ServiceUnavailable(
                        "Timeout while waiting for pipeline run result".into(),
                    )),
                }
            }
            .into_actor(self) // converts future to ActorFuture
            .map(move |result, act, _ctx| {
                // note: starting the assist session here is mostly too late, since the first pipeline event was already received while this async block was scheduled!
                act.open_requests.remove(&msg_id);

                // only keep session if pipeline run succeeded
                let error = match result {
                    Ok(response) if response.success => return Ok(()),
                    Ok(response) => map_error(response, "run pipeline"),
                    Err(e) => e,
                };

                act.assist_sessions.remove(&msg_id);

                Err(error)
            }),
        )
    }
}

impl Handler<CallListAssistPipelines> for HomeAssistantClient {
    type Result = ResponseActFuture<Self, Result<GetPipelinesResult, ServiceError>>;

    fn handle(&mut self, msg: CallListAssistPipelines, ctx: &mut Self::Context) -> Self::Result {
        let msg_id = self.new_msg_id();
        let (tx, rx) = oneshot::channel();
        self.open_requests.insert(msg_id, OpenRequest::new(tx));

        let assist_pipeline_req =
            AssistPipelineRequest::GetPipelines(AssistPipelineMsg::new(msg_id));

        if let Err(e) = self.send_json(
            serde_json::to_value(assist_pipeline_req).expect("BUG serializing"),
            ctx,
        ) {
            self.assist_sessions.remove(&msg_id);
            self.open_requests.remove(&msg_id);
            return_fut_err!(e);
        };

        Box::pin(
            async move {
                let request_timeout = Duration::from_secs(5);
                match tokio::time::timeout(request_timeout, rx).await {
                    Ok(Ok(r)) => Ok(r),
                    _ => Err(ServiceError::ServiceUnavailable(
                        "Timeout while waiting for pipeline list result".into(),
                    )),
                }
            }
            .into_actor(self) // converts future to ActorFuture
            .map(move |result, act, _ctx| {
                act.open_requests.remove(&msg_id);
                match result {
                    Ok(response) if response.success => {
                        if let Some(result) =
                            response.msg.as_object().and_then(|map| map.get("result"))
                        {
                            let mut result: GetPipelinesResult =
                                serde_json::from_value(result.clone())?;
                            // filter out non-speech capable assist pipelines if required
                            if msg.stt_required {
                                result.pipelines.retain(|p| {
                                    p.stt_engine
                                        .as_ref()
                                        .map(|stt| !stt.is_empty())
                                        .unwrap_or_default()
                                });
                                // make sure the preferred pipeline is still valid
                                if let Some(preferred) = result.preferred_pipeline.as_deref()
                                    && !result.pipelines.iter().any(|p| p.id == preferred)
                                {
                                    warn!(
                                        "Preferred assist pipeline {preferred} not found, resetting"
                                    );
                                    result.preferred_pipeline = None;
                                }
                            }
                            Ok(result)
                        } else {
                            error!("Unexpected list assist pipelines response: {:?}", response);
                            Err(ServiceError::InternalServerError(
                                "Unexpected list assist pipelines response".into(),
                            ))
                        }
                    }
                    Ok(response) => Err(map_error(response, "list assist pipelines")),
                    Err(e) => Err(e),
                }
            }),
        )
    }
}

fn map_error(response: ResponseMsg, action: &str) -> ServiceError {
    if let Some(map) = response
        .msg
        .as_object()
        .and_then(|map| map.get("error"))
        .and_then(|v| v.as_object())
    {
        let code = map.get("code").and_then(|v| v.as_str());
        let msg = map.get("message").and_then(|v| v.as_str());
        match code {
            Some("pipeline-not-found") => {
                return ServiceError::NotFound("Pipeline not found".into());
            }
            Some(code) => {
                return ServiceError::ServiceUnavailable(format!(
                    "Pipeline error {code}: {}",
                    msg.unwrap_or_default()
                ));
            }
            _ => {}
        }
    }

    ServiceError::ServiceUnavailable(format!("Failed to {action}"))
}
