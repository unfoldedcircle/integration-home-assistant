// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Configuration settings read from the configuration file.

use config::Config;
use std::time::Duration;

use log::warn;
use serde_with::{serde_as, DurationMilliSeconds, DurationSeconds};
use url::Url;

const DEF_CONNECTION_TIMEOUT: u8 = 3;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Settings {
    pub integration: IntegrationSettings,
    pub hass: HomeAssistantSettings,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct IntegrationSettings {
    pub interface: String,
    pub http: WebServerSettings,
    pub https: WebServerSettings,
    pub certs: Option<CertificateSettings>,
    pub websocket: Option<WebSocketSettings>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct WebServerSettings {
    pub enabled: bool,
    pub port: u16,
}

#[derive(Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct CertificateSettings {
    pub public: String,
    pub private: String,
}

#[derive(Default, Clone, serde::Deserialize, serde::Serialize)]
pub struct WebSocketSettings {
    pub token: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize)]
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
#[derive(serde::Deserialize, serde::Serialize)]
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
#[derive(Clone, serde::Deserialize, serde::Serialize)]
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
            integration: IntegrationSettings {
                interface: "0.0.0.0".to_string(),
                http: WebServerSettings {
                    enabled: true,
                    port: 8000,
                },
                https: WebServerSettings {
                    enabled: false, // TODO https should be the default, but not yet implemented
                    port: 8443,
                },
                certs: None,
                websocket: None,
            },
            hass: HomeAssistantSettings {
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

pub fn get_configuration(filename: Option<&str>) -> Result<Settings, config::ConfigError> {
    // default configuration
    let mut config = Config::builder().add_source(Config::try_from(&Settings::default())?);
    // read optional configuration file to override defaults
    if let Some(filename) = filename {
        config = config.add_source(config::File::with_name(filename));
    }
    // Add in settings from the environment (with a prefix of UC)
    // Eg.. `UC_HASS_URL=http://localhost:8123/api/websocket` would set the `hass.url` key
    // This does NOT WORK for nested configurations! https://github.com/mehcode/config-rs/issues/312
    let config = config
        .add_source(config::Environment::with_prefix("UC").separator("_"))
        .build()?;

    let settings: Settings = config.try_deserialize()?;

    check_cfg_values(settings)
}

fn check_cfg_values(mut settings: Settings) -> Result<Settings, config::ConfigError> {
    if settings.hass.reconnect.backoff_factor < 1.0
        || settings.hass.reconnect.duration.as_millis() < 100
        || settings.hass.reconnect.duration_max.as_millis() < 1000
    {
        warn!("Invalid HA reconnect settings, using defaults.");
        settings.hass.reconnect = Default::default();
    }

    if settings.hass.heartbeat.interval.as_secs() < 5
        || settings.hass.heartbeat.timeout.as_secs() < 5
        || settings.hass.heartbeat.timeout.as_secs() <= settings.hass.heartbeat.interval.as_secs()
    {
        warn!("Invalid HA heartbeat settings, using defaults.");
        settings.hass.heartbeat = Default::default();
    }

    match settings.hass.url.scheme() {
        "ws" | "wss" => {}
        "http" => settings.hass.url.set_scheme("ws").unwrap(),
        "https" => settings.hass.url.set_scheme("wss").unwrap(),
        scheme => {
            return Err(config::ConfigError::Message(format!(
                "invalid scheme in home_assistant.url: {}. Valid: [ws, wss]",
                scheme
            )))
        }
    }

    Ok(settings)
}
