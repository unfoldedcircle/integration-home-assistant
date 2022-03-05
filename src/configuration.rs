// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use log::warn;
use serde_with::{serde_as, DurationMilliSeconds, DurationSeconds};
use std::time::Duration;

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
    pub url: String,
    pub token: String,
    /// WebSocket connection timeout in seconds
    pub connection_timeout: u8,
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
                url: "ws://hassio.local:8123/api/websocket".to_string(),
                token: "".to_string(),
                connection_timeout: DEF_CONNECTION_TIMEOUT,
                reconnect: Default::default(),
                heartbeat: Default::default(),
            },
        }
    }
}

pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    let mut cfg = config::Config::default();
    cfg.merge(config::File::with_name("configuration"))?;
    let mut settings: Settings = cfg.try_into()?;

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

    Ok(settings)
}
