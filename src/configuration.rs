// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use config::Config;
use std::time::Duration;

use log::warn;
use serde_with::{serde_as, DurationMilliSeconds, DurationSeconds};
use url::Url;

const DEF_CONNECTION_TIMEOUT: u8 = 3;

#[derive(serde::Deserialize)]
pub struct Settings {
    pub webserver: WebServerSettings,
    pub home_assistant: HomeAssistantSettings,
}

#[derive(serde::Deserialize)]
pub struct WebServerSettings {
    pub interface: String,
    pub http: bool,
    pub http_port: u16,
    pub https: bool,
    pub https_port: u16,
    pub certs: Option<CertificateSettings>,
    pub websocket: Option<WebSocketSettings>,
}

#[derive(serde::Deserialize)]
pub struct CertificateSettings {
    pub public: String,
    pub private: String,
}

#[derive(Default, serde::Deserialize)]
pub struct WebSocketSettings {
    pub token: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct HomeAssistantSettings {
    pub url: Url,
    pub token: String,
    /// WebSocket connection timeout in seconds
    pub connection_timeout: u8,
    pub max_frame_size_kb: usize,
    pub reconnect: ReconnectSettings,
    pub heartbeat: HeartbeatSettings,
}

#[serde_as]
#[derive(serde::Deserialize)]
pub struct ReconnectSettings {
    pub attempts: u16,
    #[serde_as(as = "DurationMilliSeconds")]
    #[serde(rename = "duration_ms")]
    pub duration: Duration,
    #[serde_as(as = "DurationMilliSeconds")]
    #[serde(rename = "duration_max_ms")]
    pub duration_max: Duration,
    pub backoff_factor: f32,
}

impl Default for ReconnectSettings {
    fn default() -> Self {
        Self {
            attempts: 5,
            duration: Duration::from_secs(1),
            duration_max: Duration::from_secs(30),
            backoff_factor: 1.5,
        }
    }
}

#[serde_as]
#[derive(Clone, serde::Deserialize)]
pub struct HeartbeatSettings {
    /// How often heartbeat pings are sent
    #[serde_as(as = "DurationSeconds")]
    #[serde(rename = "interval_sec")]
    pub interval: Duration,
    /// How long before lack of server response causes a timeout
    #[serde_as(as = "DurationSeconds")]
    #[serde(rename = "timeout_sec")]
    pub timeout: Duration,
}

impl Default for HeartbeatSettings {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(20),
            timeout: Duration::from_secs(40),
        }
    }
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            webserver: WebServerSettings {
                interface: "0.0.0.0".to_string(),
                http: false,
                http_port: 8000,
                https: true,
                https_port: 8443,
                certs: None,
                websocket: None,
            },
            home_assistant: HomeAssistantSettings {
                url: Url::parse("ws://hassio.local:8123/api/websocket").unwrap(),
                token: "".to_string(),
                connection_timeout: DEF_CONNECTION_TIMEOUT,
                max_frame_size_kb: 1024,
                reconnect: Default::default(),
                heartbeat: Default::default(),
            },
        }
    }
}

pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    let config = Config::builder()
        .add_source(config::File::with_name("configuration"))
        // Add in settings from the environment (with a prefix of UC_HASS)
        // Eg.. `UC_HASS_DEBUG=1 ./target/app` would set the `debug` key
        .add_source(config::Environment::with_prefix("UC_HASS"))
        .build()?;

    let mut settings: Settings = config.try_deserialize()?;
    if settings.home_assistant.reconnect.backoff_factor < 1.0
        || settings.home_assistant.reconnect.duration.as_millis() < 100
        || settings.home_assistant.reconnect.duration_max.as_millis() < 1000
    {
        warn!("Invalid HA reconnect settings, using defaults.");
        settings.home_assistant.reconnect = Default::default();
    }

    if settings.home_assistant.heartbeat.interval.as_secs() < 5
        || settings.home_assistant.heartbeat.timeout.as_secs() < 5
        || settings.home_assistant.heartbeat.timeout.as_secs()
            <= settings.home_assistant.heartbeat.interval.as_secs()
    {
        warn!("Invalid HA heartbeat settings, using defaults.");
        settings.home_assistant.heartbeat = Default::default();
    }

    match settings.home_assistant.url.scheme() {
        "ws" | "wss" => {}
        "http" => settings.home_assistant.url.set_scheme("ws").unwrap(),
        "https" => settings.home_assistant.url.set_scheme("wss").unwrap(),
        scheme => {
            return Err(config::ConfigError::Message(format!(
                "invalid scheme in home_assistant.url: {}. Valid: [ws, wss]",
                scheme
            )))
        }
    }

    Ok(settings)
}
