// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! HA WebSocket data structure definitions for JSON serialization & deserialization.

use derive_builder::Builder;
use derive_more::Constructor;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tokio::sync::oneshot;

/// Internal message for handling a received WebSocket response message
#[derive(Debug)]
#[allow(dead_code)] // id not yet used
pub(super) struct ResponseMsg {
    /// Message ID
    pub id: u32,
    pub success: bool,
    /// JSON response message
    pub msg: serde_json::Value,
}

pub(super) struct OpenRequest {
    /// Tokio oneshot sender to notify the waiting receiver with the response message code.
    pub tx: Option<oneshot::Sender<ResponseMsg>>,
    /// Timestamp when the request was sent.
    pub ts: Instant,
}

impl OpenRequest {
    pub fn new(tx: oneshot::Sender<ResponseMsg>) -> Self {
        Self {
            tx: Some(tx),
            ts: Instant::now(),
        }
    }
}

#[allow(dead_code)]
pub(super) struct AssistSession {
    /// HA message request ID.
    pub req_id: u32,
    pub entity_id: String,
    /// Remote audio session ID.
    pub session_id: u32,
    /// HA audio chunk handler ID for binary WS messages.
    pub stt_binary_handler_id: Option<u8>,
    /// Timestamp when the session was opened.
    pub ts: Instant,
    /// Flag indicating whether the `run-end` event was received from HA.
    pub run_end: Option<Instant>,
    /// Last received error event from HA.
    pub error: Option<EventError>,
}

impl AssistSession {
    pub fn new(req_id: u32, entity_id: String, session_id: u32) -> Self {
        Self {
            req_id,
            entity_id,
            session_id,
            stt_binary_handler_id: None,
            ts: Instant::now(),
            run_end: None,
            error: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct CallServiceMsg {
    pub id: u32,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub domain: String,
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_data: Option<serde_json::Value>,
    pub target: Target,
}

#[derive(Debug, Serialize)]
pub(super) struct Target {
    pub entity_id: String,
}

/// Home Assistant entity event message.
#[derive(Debug, Deserialize)]
pub(super) struct Event {
    //pub event_type: String,  // not used, we only listen to `state_changed`
    pub data: EventData,
    // other properties omitted, not required
}

#[derive(Debug, Deserialize)]
pub(super) struct EventData {
    pub entity_id: String,
    pub new_state: EventState,
}

#[derive(Debug, Deserialize)]
pub(super) struct EventState {
    pub state: String,
    pub attributes: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Home Assistant assist pipeline event message.
///
/// A separate event definition is required from [Event] because of different properties: `event_type` vs `type` in the `event` object.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
pub enum AssistPipelineEvent {
    /// Start of pipeline run. Always emitted.
    #[serde(rename = "run-start")]
    RunStart { data: RunStart },
    /// End of pipeline run. Always emitted.
    #[serde(rename = "run-end")]
    RunEnd,
    /// Start of wake word detection
    #[serde(rename = "wake_word-start")]
    WakeWordStart, // payload omitted
    /// End of wake word detection
    #[serde(rename = "wake_word-end")]
    WakeWordEnd, // payload omitted
    /// Start of speech to text
    #[serde(rename = "stt-start")]
    SttStart { data: SttStart },
    /// Start of voice command
    #[serde(rename = "stt-vad-start")]
    SttVadStart, // payload omitted
    /// End of voice command
    #[serde(rename = "stt-vad-end")]
    SttVadEnd, // payload omitted
    /// End of speech to text
    #[serde(rename = "stt-end")]
    SttEnd { data: SttEnd },
    /// Start of intent recognition. Always emitted.
    #[serde(rename = "intent-start")]
    IntentStart { data: IntentStart },
    /// Intermediate update of intent recognition
    #[serde(rename = "intent-progress")]
    IntentProgress, // payload omitted
    /// End of intent recognition. Always emitted.
    #[serde(rename = "intent-end")]
    IntentEnd { data: IntentEnd },
    /// Start of text to speech
    #[serde(rename = "tts-start")]
    TtsStart { data: TtsStart },
    /// End of text to speech
    #[serde(rename = "tts-end")]
    TtsEnd { data: TtsEnd },
    /// Error in pipeline
    #[serde(rename = "error")]
    Error { data: EventError },
}

/// Start of pipeline run
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RunStart {
    pub pipeline: String,
    #[serde(default = "default_pipeline_language")]
    pub language: String,
    // not sure if optional, not clear from HA docs
    pub runner_data: Option<RunnerData>,
    pub tts_output: Option<TtsOutput>,
}

fn default_pipeline_language() -> String {
    "en".to_string()
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SttStart {
    pub engine: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct TtsEnd {
    // documentation mismatch in HA: better make it optional
    pub tts_output: Option<TtsOutput>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RunnerData {
    /// The prefix to send speech data over.
    pub stt_binary_handler_id: u8,
    /// The max run time for the whole pipeline.
    pub timeout: u16,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct TtsOutput {
    pub token: String,
    pub url: String,
    pub mime_type: String,
    pub stream_response: Option<bool>,
    pub media_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SttEnd {
    pub stt_output: Option<SttOutput>,
}

#[derive(Debug, Deserialize)]
pub struct SttOutput {
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct IntentStart {
    pub conversation_id: Option<String>,
    pub device_id: Option<String>,
    pub engine: String,
    pub language: String,
    pub intent_input: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct IntentEnd {
    pub processed_locally: Option<bool>,
    pub intent_output: Option<IntentOutput>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct IntentOutput {
    pub conversation_id: Option<String>,
    pub continue_conversation: Option<bool>,
    pub response: Option<ConversationResponse>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ConversationResponse {
    pub response_type: ResponseType,
    pub language: String,
    pub speech: Option<serde_json::Value>,
}

#[derive(Debug, Copy, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseType {
    #[default]
    Unknown,
    ActionDone,
    QueryAnswer,
    Error,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct TtsStart {
    pub engine: String,
    pub language: String,
    pub voice: String,
    pub tts_input: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EventError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Copy, Clone, Default, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum StartStage {
    WakeWord,
    #[default]
    Stt,
    Intent,
    Tts,
}

#[derive(Debug, Copy, Clone, Default, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum EndStage {
    Stt,
    #[default]
    Intent,
    Tts,
}

/// Home Assistant `assist_pipeline/*` request.
///
/// See <https://github.com/home-assistant/core/blob/dev/homeassistant/components/assist_pipeline/websocket_api.py>
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
pub(super) enum AssistPipelineRequest {
    #[serde(rename = "assist_pipeline/pipeline/list")]
    GetPipelines(AssistPipelineMsg),
    #[serde(rename = "assist_pipeline/run")]
    Run(RunAssistPipeline),
    #[serde(rename = "assist_pipeline/language/list")]
    GetLanguages(AssistPipelineMsg),
    #[serde(rename = "assist_pipeline/device/list")]
    GetDevices(AssistPipelineMsg),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GetPipelinesResult {
    pub pipelines: Vec<AssistPipelineCfg>,
    pub preferred_pipeline: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AssistPipelineCfg {
    pub id: String,
    pub language: String,
    pub name: String,
    pub stt_engine: Option<String>,
    pub tts_engine: Option<String>,
}

/// See <https://developers.home-assistant.io/docs/voice/pipelines/>
#[derive(Builder, Debug, Serialize)]
pub(super) struct RunAssistPipeline {
    /// Message request ID.
    id: u32,
    /// The first stage to run.
    #[builder(default)]
    start_stage: StartStage,
    /// The last stage to run.
    #[builder(default)]
    end_stage: EndStage,
    /// Depends on start_stage
    input: serde_json::Value,
    /// Optional. ID of the pipeline (use `assist_pipeline/pipeline/list` to get names).
    #[builder(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pipeline: Option<String>,
    /// Optional. [Unique id for conversation](https://developers.home-assistant.io/docs/intent_conversation_api#conversation-id).
    #[builder(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    conversation_id: Option<String>,
    /// Optional. Device ID from Home Assistant's device registry of the device that is starting the pipeline.
    #[builder(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<String>,
    /// Optional. Number of seconds before pipeline times out (default: 300).
    #[builder(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u16>,
}

/// Simple request message without payload.
#[derive(Debug, Constructor, Serialize)]
pub(super) struct AssistPipelineMsg {
    /// Message request ID.
    pub id: u32,
}
