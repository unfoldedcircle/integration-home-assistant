// Copyright (c) 2025 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home-Assistant voice command test tool

use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, ResponseFuture, fut};
use actix_web::rt::time::sleep;
use bytes::{BufMut, BytesMut};
use clap::Parser;
use log::{error, info};
use std::cmp::min;
use std::env;
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;
use uc_api::EntityType;
use uc_api::intg::ws::{DriverEvent, R2Event, R2Request};
use uc_api::intg::{AssistantEvent, IntgVoiceAssistantCommand};
use uc_intg_hass::configuration::{ENV_HASS_MSG_TRACING, Settings, get_configuration};
use uc_intg_hass::{
    Controller, NewR2Session, R2AudioChunkMsg, R2EventMsg, R2RequestMsg, SendWsMessage,
    configuration,
};
use url::Url;

const HA_AUDIO_STREAM_CHUNK_SIZE: usize = 4096;
const HA_ASSIST_PIPELINE_TIMEOUT_SEC: u16 = 10;

#[derive(Parser, Debug)]
#[command(version, about = "Home Assistant voice command test", long_about = None, author = "Unfolded Circle ApS")]
pub struct Opt {
    /// Home Assistant WebSocket API URL (overrides home-assistant.json).
    #[arg(short, long)]
    pub url: Option<String>,
    /// Disable SSL certificate verification (overrides home-assistant.json).
    #[arg(long, num_args = 0)]
    pub disable_cert_validation: Option<bool>,
    /// "Home Assistant long-lived access token (overrides home-assistant.json).
    #[arg(short, long)]
    pub token: Option<String>,
    /// WAV file containing voice command. Must be mono, 16 kHz, 16-bit signed PCM.
    #[arg(short, long, default_value_t = String::from("voice_command.wav"))]
    pub audio_file: String,
    /// Request speech response from the Assist pipeline.
    #[arg(short, long, default_value_t = false)]
    pub speech_response: bool,
    /// TCP connection timeout in seconds (overrides home-assistant.json).
    #[arg(short, long)]
    pub connection_timeout: Option<u8>,
    /// Request timeout in seconds (overrides home-assistant.json).
    #[arg(short, long)]
    pub request_timeout: Option<u8>,
    /// Message tracing for HA server communication.
    #[arg(long, value_name = "MESSAGES", value_parser = ["in", "out", "all", "none"], default_value = "all")]
    pub trace_level: Option<String>,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    let opt = Opt::parse();
    let voice_file = PathBuf::from(&opt.audio_file);
    let speech_response = opt.speech_response;
    let cfg = parse_args_load_cfg(opt)?;

    println!(
        "Connecting to Home Assistant WebSocket server: {} (timeout={}s, request-timeout={}s, disable-cert={})",
        cfg.hass.get_url(),
        cfg.hass.connection_timeout,
        cfg.hass.request_timeout,
        cfg.hass.disable_cert_validation,
    );

    let driver_metadata = configuration::get_driver_metadata()?;
    let controller = Controller::new(cfg, driver_metadata.clone()).start();

    // Mock server to simulate an R2 connection
    let ws_id = "HA-test".to_string();
    let server = ServerMock::new(&ws_id, voice_file, speech_response, controller.clone())?.start();

    // establish a mock session
    controller
        .send(NewR2Session {
            addr: server.recipient(),
            id: ws_id.clone(),
        })
        .await?;

    // connect to HA
    controller
        .send(R2EventMsg {
            ws_id: ws_id.clone(),
            event: R2Event::Connect,
            msg_data: None,
        })
        .await?;

    // quick and dirty for now
    sleep(Duration::from_secs(30)).await;

    Ok(())
}

fn parse_args_load_cfg(opt: Opt) -> anyhow::Result<Settings> {
    if let Some(msg_trace) = opt.trace_level {
        unsafe {
            env::set_var(ENV_HASS_MSG_TRACING, msg_trace);
        }
    }

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .unwrap();

    let cfg_file = None;
    let mut cfg = get_configuration(cfg_file).expect("Failed to read configuration");
    if let Some(url) = opt.url {
        cfg.hass.set_url(Url::parse(&url)?);
    }
    if let Some(disable_cert_validation) = opt.disable_cert_validation {
        cfg.hass.disable_cert_validation = disable_cert_validation;
    }
    if let Some(token) = opt.token {
        cfg.hass.set_token(token);
    }
    if let Some(timeout) = opt.connection_timeout {
        cfg.hass.connection_timeout = timeout;
    }
    if let Some(timeout) = opt.request_timeout {
        cfg.hass.request_timeout = timeout;
    }

    if !cfg.hass.get_url().has_host() || cfg.hass.get_token().is_empty() {
        eprintln!("Can't connect to Home Assistant: URL or token is missing");
        exit(1);
    }

    Ok(cfg)
}

struct ServerMock {
    id: String,
    connected: bool,
    controller_addr: Addr<Controller>,
    sample_rate: u32,
    voice_buffer: BytesMut,
    speech_response: bool,
}

impl ServerMock {
    fn new(
        id: impl Into<String>,
        voice_file: PathBuf,
        speech_response: bool,
        controller_addr: Addr<Controller>,
    ) -> anyhow::Result<Self> {
        let mut reader = hound::WavReader::open(&voice_file)?;
        let spec = reader.spec();
        if spec.channels != 1 {
            eprintln!("Audio file must be mono");
            exit(1);
        }
        if spec.sample_rate != 16000 && spec.sample_rate != 8000 {
            eprintln!("Audio file must be 8 or 16 kHz");
            exit(1);
        }
        if spec.bits_per_sample != 16 {
            eprintln!("Audio file must be 16-bit signed PCM");
            exit(1);
        }
        if spec.sample_format != hound::SampleFormat::Int {
            eprintln!("Audio file must be signed 16-bit PCM");
            exit(1);
        }

        let mut buffer = BytesMut::with_capacity(reader.len() as usize);
        reader
            .samples::<i16>()
            .for_each(|sample| buffer.put_i16_le(sample.unwrap()));

        Ok(Self {
            id: id.into(),
            connected: false,
            controller_addr,
            sample_rate: spec.sample_rate,
            voice_buffer: buffer,
            speech_response,
        })
    }
}

impl Actor for ServerMock {
    type Context = Context<Self>;
}

impl Handler<SendWsMessage> for ServerMock {
    type Result = ResponseFuture<()>;

    fn handle(&mut self, msg: SendWsMessage, ctx: &mut Self::Context) -> Self::Result {
        // only interested in events
        if msg.0.kind.as_deref() != Some("event") {
            return Box::pin(fut::ready(()));
        }

        let msg_name = msg.0.msg.clone().unwrap_or_default();

        // wait for the CONNECTED event to initiate voice command
        if msg_name == "device_state"
            && let Some(msg_data) = msg.0.msg_data.as_ref()
        {
            self.connected = msg_data
                .as_object()
                .and_then(|o| o.get("state"))
                .and_then(|s| s.as_str())
                == Some("CONNECTED");

            // start voice command to HA, then wait for events
            if self.connected {
                let request = R2RequestMsg {
                    ws_id: self.id.clone(),
                    req_id: 0,
                    request: R2Request::EntityCommand,
                    msg_data: Some(serde_json::json!({
                        "entity_id": "va-test",
                        "entity_type": EntityType::VoiceAssistant.as_ref(),
                        "cmd_id": IntgVoiceAssistantCommand::VoiceStart.as_ref(),
                        "params": {
                            "session_id": 456,
                            "audio_cfg": {
                                "sample_rate": self.sample_rate,
                            },
                            "speech_response": self.speech_response,
                            "timeout": HA_ASSIST_PIPELINE_TIMEOUT_SEC
                        }
                    })),
                };
                let controller_addr = self.controller_addr.clone();
                return Box::pin(async move {
                    match controller_addr.send(request).await {
                        Ok(Ok(_)) => return,
                        Ok(Err(e)) => error!("Assist pipeline call failed: {e}"),
                        Err(e) => error!("Failed to send voice command: {e}"),
                    }
                    exit(1);
                });
            }
        } else if msg_name == DriverEvent::AssistantEvent.as_ref()
            && let Some(msg_data) = msg.0.msg_data.as_ref()
            && let Ok(event) = serde_json::from_value::<AssistantEvent>(msg_data.clone())
        {
            info!("Got voice event: {:?}", event);

            match event {
                AssistantEvent::Ready { session_id, .. } => {
                    info!("Voice assistant is ready, starting audio stream...");

                    ctx.notify_later(SendAudioChunk(session_id), Duration::from_millis(10));
                }
                AssistantEvent::SttResponse { .. } => {}
                AssistantEvent::TextResponse { .. } => {}
                AssistantEvent::SpeechResponse { .. } => {}
                AssistantEvent::Finished { .. } => {
                    // Note: error event can be sent AFTER the run-end event!
                    tokio::spawn(async {
                        sleep(Duration::from_secs(1)).await;
                        exit(0);
                    });
                }
                AssistantEvent::Error { .. } => {}
            }
        }

        Box::pin(fut::ready(()))
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct SendAudioChunk(u32);

/// Send an audio chunk every 100 ms until the audio buffer is empty.
impl Handler<SendAudioChunk> for ServerMock {
    type Result = ResponseFuture<()>;

    fn handle(&mut self, msg: SendAudioChunk, ctx: &mut Self::Context) -> Self::Result {
        let session_id = msg.0;
        let chunk = if self.voice_buffer.is_empty() {
            info!("Voice buffer is empty, sending FINISHED event...");
            BytesMut::new()
        } else {
            let len = min(self.voice_buffer.len(), HA_AUDIO_STREAM_CHUNK_SIZE);
            self.voice_buffer.split_to(len)
        };

        let controller_addr = self.controller_addr.clone();
        let addr = ctx.address();
        Box::pin(async move {
            let len = chunk.len();
            info!("Sending voice buffer of length {len} bytes...");
            let msg = R2AudioChunkMsg::new(session_id, chunk.freeze());

            if let Err(e) = controller_addr.send(msg).await {
                error!("Failed to send voice buffer: {e}");
            } else if len > 0 {
                sleep(Duration::from_millis(100)).await;
                addr.do_send(SendAudioChunk(session_id));
            }
        })
    }
}
