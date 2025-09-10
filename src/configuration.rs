// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Configuration file handling.

use crate::APP_VERSION;
use crate::errors::ServiceError;
use config::Config;
use log::{error, info, warn};
use serde_with::{DurationMilliSeconds, DurationSeconds, serde_as};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs, io};
use uc_api::intg::IntegrationDriverUpdate;
use url::Url;

/// Default configuration file.
pub const DEF_CONFIG_FILE: &str = "configuration.yaml";

pub const DEF_HA_URL: &str = "ws://homeassistant.local:8123/api/websocket";

pub const ENV_SETUP_TIMEOUT: &str = "UC_SETUP_TIMEOUT";
pub const DEF_SETUP_TIMEOUT_SEC: u64 = 300;

const ENV_USER_CFG_FILENAME: &str = "UC_USER_CFG_FILENAME";
const DEV_USER_CFG_FILENAME: &str = "home-assistant.json";

/// Environment variable for the user configuration directory.
///
/// This ENV variable is set on the Remote device to the integration specific data directory.
const ENV_CONFIG_HOME: &str = "UC_CONFIG_HOME";

/// Environment variable for the credential files directory.
const ENV_TOKENS_HOME: &str = "UC_TOKENS_HOME";

/// External system `token_id` value holding the HA server url and access token.
const TOKEN_ID: &str = "ws-ha-api";

/// Environment variable to disable mDNS service publishing.
///
/// When running on the Remote device, service publishing is not required.
pub const ENV_DISABLE_MDNS_PUBLISH: &str = "UC_DISABLE_MDNS_PUBLISH";

/// Environment variable to enable Home Assistant server WebSocket message tracing.
///
/// Valid values:
/// - `all`: enable incoming and outgoing message traces
/// - `in`: only incoming messages
/// - `out`: only outgoing messages
///
/// **Attention:** this setting is only for debugging and exposes all data, including credentials!
pub const ENV_HASS_MSG_TRACING: &str = "UC_HASS_MSG_TRACING";

/// Environment variable to enable Remote Two Integration API WebSocket message tracing.
///
/// Valid values:
/// - `all`: enable incoming and outgoing message traces
/// - `in`: only incoming messages
/// - `out`: only outgoing messages
///
/// **Attention:** this setting is only for debugging and exposes all data, including credentials!
pub const ENV_API_MSG_TRACING: &str = "UC_API_MSG_TRACING";

/// Environment variable to disable TLS verification to the Home Assistant server.
pub const ENV_DISABLE_CERT_VERIFICATION: &str = "UC_DISABLE_CERT_VERIFICATION";

/// Compiled-in driver metadata in json format.
const DRIVER_METADATA: &str = include_str!("../resources/driver.json");

#[derive(Default, serde::Deserialize, serde::Serialize)]
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

impl Default for IntegrationSettings {
    fn default() -> Self {
        Self {
            interface: "0.0.0.0".to_string(),
            http: WebServerSettings {
                enabled: true,
                port: 8000,
            },
            https: WebServerSettings {
                enabled: false, // requires user provided certificate
                port: 9443,
            },
            certs: None,
            websocket: None,
        }
    }
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
    pub heartbeat: HeartbeatSettings,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct HomeAssistantSettings {
    url: Url,
    token: String,
    /// WebSocket connection timeout in seconds.
    /// This is the max time allowed to connect to the remote host, including DNS name resolution.
    /// Make sure that `request_timeout` >= `connection_timeout`.
    pub connection_timeout: u8,
    /// WebSocket request timeout in seconds.
    /// This is the total time before a response must be received. Should be equal or greater than `connection_timeout`.
    // simplifies data migration: missing value in existing configuration will be set with a default!
    #[serde(default = "default_request_timeout")]
    pub request_timeout: u8,
    pub max_frame_size_kb: usize,
    pub reconnect: ReconnectSettings,
    pub heartbeat: HeartbeatSettings,
    /// Disconnect WebSocket connection when remote enters standby.
    /// Should be enabled if running on the device, disabled for an external integration.
    // for data migration of existing configurations
    #[serde(default = "default_disconnect_in_standby")]
    pub disconnect_in_standby: bool,
    /// Disables certificate verification for the Home Assistant WS connection.
    // for data migration of existing configurations
    #[serde(default = "default_disable_cert_validation")]
    pub disable_cert_validation: bool,
}

impl Default for HomeAssistantSettings {
    fn default() -> Self {
        Self {
            url: Url::parse(DEF_HA_URL).unwrap(),
            token: "".to_string(),
            connection_timeout: 6,
            request_timeout: default_request_timeout(),
            max_frame_size_kb: 5120,
            reconnect: Default::default(),
            heartbeat: Default::default(),
            disconnect_in_standby: default_disconnect_in_standby(),
            disable_cert_validation: default_disable_cert_validation(),
        }
    }
}

impl HomeAssistantSettings {
    /// Checks if an external URL and token has been provided.
    ///
    /// See Core-API: `/auth/external/:system`
    pub fn has_external_url_and_token(&self) -> bool {
        self.get_token_value(TOKEN_ID).is_some() && self.get_ext_url().is_some()
    }

    /// Return the configured HA server URL.
    ///
    /// This is either the provided URL in the external system, or the local configuration URL.
    pub fn get_url(&self) -> Url {
        if let Some(url) = self.get_ext_url() {
            return url;
        }
        self.url.clone()
    }

    fn get_ext_url(&self) -> Option<Url> {
        let key = format!("{TOKEN_ID}-URL");
        if let Some(url) = self.get_token_value(&key) {
            match Url::parse(&url) {
                Ok(url) => return Some(url),
                Err(e) => error!("Invalid URL in token '{key}': {e}"),
            }
        }
        None
    }

    /// Return the configured HA server access token.
    ///
    /// This is either the provided token in the external system, or the local configuration token.
    pub fn get_token(&self) -> String {
        self.get_token_value(TOKEN_ID)
            .unwrap_or_else(|| self.token.clone())
    }

    /// Update the local configuration URL.
    pub fn set_url(&mut self, url: Url) {
        self.url = url;
    }

    /// Update the local configuration token.
    pub fn set_token(&mut self, token: impl AsRef<str>) {
        self.token = token.as_ref().trim().to_string();
    }

    /// Get the value of an external system token key.
    ///
    /// # Arguments
    ///
    /// * `key`: token key
    ///
    /// returns: None if the token file doesn't exist or the file couldn't be read.
    fn get_token_value(&self, key: &str) -> Option<String> {
        let mut path = PathBuf::from(env::var(ENV_TOKENS_HOME).ok()?);
        path.push(key);
        if !path.is_file() {
            info!("Token file '{key}' does not exist. Using local configuration.");
            return None;
        }

        match fs::read_to_string(path) {
            Ok(v) => Some(v.trim().to_string()),
            Err(e) => {
                error!("Error reading token file '{key}', using local configuration. {e}");
                None
            }
        }
    }
}

fn default_request_timeout() -> u8 {
    6
}
fn default_disconnect_in_standby() -> bool {
    true
}
fn default_disable_cert_validation() -> bool {
    false
}

#[serde_as]
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct ReconnectSettings {
    pub attempts: u32,
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
            attempts: 0,
            duration: Duration::from_secs(1),
            duration_max: Duration::from_secs(30),
            backoff_factor: 1.5,
        }
    }
}

/// WebSocket heartbeat settings for sending ping frames.
#[serde_as]
#[derive(Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct HeartbeatSettings {
    /// Use native WebSocket ping frames instead of [API messages](https://developers.home-assistant.io/docs/api/websocket/#pings-and-pongs)
    #[serde(default)]
    pub ping_frames: bool,
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
            ping_frames: false,
            interval: Duration::from_secs(20),
            timeout: Duration::from_secs(40),
        }
    }
}

impl Display for HeartbeatSettings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Heartbeat interval={:?}, timeout={:?}, ping frames={}",
            self.interval, self.timeout, self.ping_frames
        )
    }
}

/// Load the configuration settings.
///
/// The application provides default values which can be overriden in the following order:
/// 1. Configuration settings in the read-only yaml configuration file specified in `filename`
/// 2. User provided configuration settings from the driver setup
/// 3. Environment variables with prefix `UC_` (works only for cfg keys not containing a `_`!)
///
/// If there's a configuration load error, the configuration will be reloaded without the user
/// provided configuration settings for auto-recovery with default values.
pub fn get_configuration(filename: Option<&str>) -> Result<Settings, config::ConfigError> {
    let user_config = user_settings_path();
    if !user_config.is_file() {
        info!("No user settings file found");
        return load_configuration(filename, None);
    }

    match load_configuration(filename, Some(user_config)) {
        Ok(cfg) => Ok(cfg),
        Err(e) => {
            error!("Error loading configuration, retrying without user configuration. Error: {e}");
            load_configuration(filename, None)
        }
    }
}

fn load_configuration(
    filename: Option<&str>,
    user_config: Option<PathBuf>,
) -> Result<Settings, config::ConfigError> {
    // default configuration
    let mut config = Config::builder().add_source(Config::try_from(&Settings::default())?);
    // read optional configuration file to override defaults
    if let Some(filename) = filename {
        config = config.add_source(config::File::with_name(filename));
    }

    // Overlay user provided configuration file from driver setup flow.
    if let Some(user_config) = user_config {
        config = config.add_source(config::File::from(user_config));
    }

    // Add in settings from the environment (with a prefix of UC)
    // E.g. `UC_HASS_URL=http://localhost:8123/api/websocket` would set the `hass.url` key
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
            )));
        }
    }

    Ok(settings)
}

/// Deserialize and enhance driver information from compiled-in json data.
pub fn get_driver_metadata() -> Result<IntegrationDriverUpdate, io::Error> {
    let mut driver: IntegrationDriverUpdate =
        serde_json::from_str(DRIVER_METADATA).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid driver.json format: {e}"),
            )
        })?;

    if driver.driver_id.is_none() {
        driver.driver_id = Some("home-assistant".into())
    }
    if !driver
        .name
        .as_ref()
        .map(|v| !v.is_empty())
        .unwrap_or_default()
    {
        driver.name = Some(HashMap::from([("en".into(), "Home Assistant".into())]))
    }
    driver.token = None; // don't expose sensitive information
    driver.version = Some(APP_VERSION.to_string());

    Ok(driver)
}

/// Wrapper to add the `hass` root property to make it compatible with the main configuration file.
#[derive(serde::Deserialize, serde::Serialize)]
struct UserSettingsWrapper {
    hass: HomeAssistantSettings,
}

/// Store user configuration from the setup flow.
pub fn save_user_settings(cfg: &HomeAssistantSettings) -> Result<(), ServiceError> {
    let cfg = UserSettingsWrapper { hass: cfg.clone() };
    fs::write(user_settings_path(), serde_json::to_string_pretty(&cfg)?).map_err(|e| {
        let msg = format!("Error saving user configuration: {e}");
        error!("{msg}");
        ServiceError::InternalServerError(msg)
    })?;
    Ok(())
}

/// Get user configuration file path.
///
/// This configuration file is updatable with [`save_user_settings`] from the driver setup flow.
///
/// The configuration file is located in the configuration directory specified in the env variable
/// `UC_CONFIG_HOME`. If not set, the current directory is used.
fn user_settings_path() -> PathBuf {
    let file = env::var(ENV_USER_CFG_FILENAME).unwrap_or(DEV_USER_CFG_FILENAME.into());
    Path::new(&env::var(ENV_CONFIG_HOME).unwrap_or_default()).join(file)
}
