// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Driver setup flow handling.

use crate::configuration::save_user_settings;
use crate::controller::handler::{
    AbortDriverSetup, ConnectMsg, SetDriverUserDataMsg, SetupDriverMsg,
};
use crate::controller::{Controller, OperationModeInput::*, OperationModeState};
use crate::errors::{ServiceError, ServiceError::BadRequest};
use actix::clock::sleep;
use actix::{fut, ActorFutureExt, AsyncContext, Handler, Message, ResponseActFuture, WrapFuture};
use derive_more::Constructor;
use log::{debug, info, warn};
use serde_json::json;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use uc_api::intg::{DriverSetupChange, IntegrationSetup};
use uc_api::model::intg::{IntegrationSetupError, IntegrationSetupState, SetupChangeEventType};
use uc_api::ws::{EventCategory, WsMessage};
use url::Url;

/// Local Actix message to request further user data.
#[derive(Constructor, Message)]
#[rtype(result = "()")]
struct RequestExpertOptionsMsg {
    pub ws_id: String,
}

/// Local Actix message to finish setup flow.
#[derive(Constructor, Message)]
#[rtype(result = "()")]
struct FinishSetupFlowMsg {
    pub ws_id: String,
    pub error: Option<IntegrationSetupError>,
}

/// Start integration setup flow.
///
/// Disconnect an active HA connection to start a new client connection with the changed data later.   
/// Either continue with expert configuration options with [RequestExpertOptionsMsg] if selected in initial
/// configuration screen, or finish setup with [FinishSetupFlowMsg].
impl Handler<SetupDriverMsg> for Controller {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, msg: SetupDriverMsg, ctx: &mut Self::Context) -> Self::Result {
        debug!("[{}] {:?}", msg.ws_id, msg.data);

        if self
            .sm_consume(&msg.ws_id, &SetupDriverRequest, ctx)
            .is_err()
        {
            return Err(BadRequest(
                "Cannot start driver setup. Please abort setup first.".into(),
            ));
        }

        let mut cfg = self.settings.hass.clone();

        // validate setup data
        cfg.url = validate_url(msg.data.setup_data.get("url").map(|u| u.as_str()))?;

        if let Some(token) = msg.data.setup_data.get("token").map(|t| t.trim()) {
            if token.is_empty() && !cfg.token.is_empty() {
                warn!(
                    "[{}] no token value provided in setup, using existing token",
                    msg.ws_id
                )
            } else if !token.is_empty() {
                cfg.token = token.to_string();
            } else {
                return Err(BadRequest("Missing token".into()));
            }
        } else {
            return Err(BadRequest("Missing field: token".into()));
        }

        if let Some(session) = self.sessions.get_mut(&msg.ws_id) {
            session.reconfiguring = msg.data.reconfigure;
        };

        save_user_settings(&cfg)?;

        info!("Disconnecting from HA during setup-flow");
        self.disconnect(ctx);

        // TODO verify WebSocket connection to make sure user provided URL & token are ok! #3
        // Right now the core will just send a Connect request after setup...
        self.settings.hass = cfg;

        // use a delay that the ack response will be sent first
        let delay = Duration::from_millis(100);
        if msg
            .data
            .setup_data
            .get("expert")
            .and_then(|v| bool::from_str(v).ok())
            .unwrap_or_default()
        {
            // start expert setup with an additional configuration screen
            ctx.notify_later(RequestExpertOptionsMsg::new(msg.ws_id), delay);
        } else {
            // setup done!
            ctx.notify_later(FinishSetupFlowMsg::new(msg.ws_id, None), delay);
        }

        // this will acknowledge the setup_driver request message
        Ok(())
    }
}

/// Handle driver setup input data from the expert configuration screen.
///
/// Validate and save entered data, then trigger the end of the setup flow with [FinishSetupFlowMsg].
impl Handler<SetDriverUserDataMsg> for Controller {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, msg: SetDriverUserDataMsg, ctx: &mut Self::Context) -> Self::Result {
        debug!("[{}] {:?}", msg.ws_id, msg.data);

        if self.sm_consume(&msg.ws_id, &SetupUserData, ctx).is_err() {
            return Err(BadRequest(
                "Not waiting for driver user data. Please restart setup.".into(),
            ));
        }

        // validate setup data
        let mut cfg = self.settings.hass.clone();
        if let IntegrationSetup::InputValues(values) = msg.data {
            if let Some(value) = parse_value(&values, "connection_timeout") {
                if value >= 3 {
                    cfg.connection_timeout = value;
                }
            }
            if let Some(value) = parse_value(&values, "request_timeout") {
                if value >= 3 {
                    cfg.request_timeout = value;
                }
            }
            if let Some(value) = parse_value(&values, "disconnect_in_standby") {
                cfg.disconnect_in_standby = value;
            }
            if let Some(value) = parse_value(&values, "max_frame_size_kb") {
                if value >= 1024 {
                    cfg.max_frame_size_kb = value;
                }
            }
            if let Some(value) = parse_value(&values, "heartbeat_interval") {
                cfg.heartbeat.interval = Duration::from_secs(value);
            }
            if let Some(value) = parse_value(&values, "heartbeat_timeout") {
                cfg.heartbeat.timeout = Duration::from_secs(value);
            }
            if let Some(value) = parse_value(&values, "ping_frames") {
                cfg.heartbeat.ping_frames = value;
            }
            if let Some(value) = parse_value(&values, "reconnect.attempts") {
                cfg.reconnect.attempts = value;
            }
            if let Some(value) = parse_value(&values, "reconnect.duration_ms") {
                cfg.reconnect.duration = Duration::from_millis(value);
            }
            if let Some(value) = parse_value(&values, "reconnect.duration_max_ms") {
                cfg.reconnect.duration_max = Duration::from_millis(value);
            }
            if let Some(value) = parse_value(&values, "reconnect.backoff_factor") {
                if value >= 1f32 {
                    cfg.reconnect.backoff_factor = value;
                }
            }
        } else {
            return Err(BadRequest("Invalid response: require input_values".into()));
        }

        save_user_settings(&cfg)?;
        self.settings.hass = cfg;

        // use a delay that the ack response will be sent first
        ctx.notify_later(
            FinishSetupFlowMsg::new(msg.ws_id, None),
            Duration::from_millis(100),
        );

        // this will acknowledge the set_driver_user_data request message
        Ok(())
    }
}

/// Send the expert configuration data request.
///
/// The setup flow will continue with the [SetDriverUserDataMsg] or timeout if no response is received.
impl Handler<RequestExpertOptionsMsg> for Controller {
    type Result = ();

    fn handle(&mut self, msg: RequestExpertOptionsMsg, ctx: &mut Self::Context) -> Self::Result {
        if self.sm_consume(&msg.ws_id, &RequestUserInput, ctx).is_err() {
            return;
        }

        let event = WsMessage::event(
            "driver_setup_change",
            EventCategory::Device,
            json!({
                "event_type": SetupChangeEventType::Setup,
                "state": IntegrationSetupState::WaitUserAction,
                "require_user_action": {
                    "input": {
                        "title": {
                            "en": "Expert configuration",
                            "de": "Expert Konfiguration"
                        },
                        "settings": [
                            {
                                "id": "connection_timeout",
                                "label": {
                                    "en": "TCP connection timeout in seconds",
                                    "de": "TCP Verbindungs-Timeout in Sekunden"
                                },
                                "field": {
                                    "number": {
                                        "value": self.settings.hass.connection_timeout,
                                        "min": 3,
                                        "max": 30,
                                        "unit": { "en": "sec" } // not yet working in web-configurator
                                    }
                                }
                            },
                            {
                                "id": "request_timeout",
                                "label": {
                                    "en": "Request timeout in seconds",
                                    "de": "Anfrage-Timeout in Sekunden"
                                },
                                "field": {
                                    "number": {
                                        "value": self.settings.hass.request_timeout,
                                        "min": 3,
                                        "max": 30,
                                        "unit": { "en": "sec" }
                                    }
                                }
                            },
                            {
                                "id": "disconnect_in_standby",
                                "label": {
                                    "en": "Disconnect when entering standby",
                                    "de": "Trennen der Verbindung im Standby-Modus"
                                },
                                "field": {
                                    "checkbox": {
                                      "value": self.settings.hass.disconnect_in_standby
                                    }
                                }
                            },
                            {
                                "id": "max_frame_size_kb",
                                "label": {
                                    "en": "Max WebSocket frame size (kilobyte)",
                                    "de": "Max WebSocket Frame Grösse (Kilobyte)"
                                },
                                "field": {
                                    "number": {
                                        "value": self.settings.hass.max_frame_size_kb,
                                        "min": 1024,
                                        "max": 16384,
                                        "unit": { "en": "KB" }
                                    }
                                }
                            },
                            {
                                "id": "reconnect.attempts",
                                "label": {
                                    "en": "Max reconnect attempts (0 = unlimited)",
                                    "de": "Max Anzahl Verbindungsversuche (0 = unlimitiert)"
                                },
                                "field": {
                                    "number": {
                                        "value": self.settings.hass.reconnect.attempts,
                                        "min": 0,
                                        "max": 2000000
                                    }
                                }
                            },
                            {
                                "id": "reconnect.duration_ms",
                                "label": {
                                    "en": "Initial reconnect delay in milliseconds",
                                    "de": "Initiale Wiederverbindungsverzögerung in ms"
                                },
                                "field": {
                                    "number": {
                                        "value": self.settings.hass.reconnect.duration.as_millis(),
                                        "min": 100,
                                        "max": 600000,
                                        "unit": { "en": "ms" }
                                    }
                                }
                            },
                            {
                                "id": "reconnect.duration_max_ms",
                                "label": {
                                    "en": "Max reconnect delay in milliseconds",
                                    "de": "Max Wiederverbindungsverzögerung in ms"
                                },
                                "field": {
                                    "number": {
                                        "value": self.settings.hass.reconnect.duration_max.as_millis(),
                                        "min": 1000,
                                        "max": 600000,
                                        "unit": { "en": "ms" }
                                    }
                                }
                            },
                            {
                                "id": "reconnect.backoff_factor",
                                "label": {
                                    "en": "Reconnect backoff factor"
                                },
                                "field": {
                                    "number": {
                                        "value": self.settings.hass.reconnect.backoff_factor,
                                        "min": 1,
                                        "max": 10,
                                        "decimals": 1,
                                    }
                                }
                            },
                            {
                                "id": "heartbeat_interval",
                                "label": {
                                    "en": "Heartbeat interval in seconds (0 = disabled)",
                                    "de": "Heartbeat Intervall in Sekunden (0 = deaktiviert)"
                                },
                                "field": {
                                    "number": {
                                        "value": self.settings.hass.heartbeat.interval.as_secs(),
                                        "min": 0,
                                        "max": 60,
                                        "unit": { "en": "sec", "de": "Sek" }
                                    }
                                }
                            },
                            {
                                "id": "heartbeat_timeout",
                                "label": {
                                    "en": "Heartbeat timeout in seconds (0 = disabled)",
                                    "de": "Heartbeat Timeout in Sekunden (0 = deaktiviert)"
                                },
                                "field": {
                                    "number": {
                                        "value": self.settings.hass.heartbeat.timeout.as_secs(),
                                        "min": 0,
                                        "max": 300,
                                        "unit": { "en": "sec", "de": "Sek" }
                                    }
                                }
                            },
                            {
                                "id": "ping_frames",
                                "label": {
                                    "en": "Use WebSocket ping frames for heartbeat",
                                    "de": "Verwende WebSocket Ping-frames für Heartbeat"
                                },
                                "field": {
                                    "checkbox": {
                                      "value": self.settings.hass.heartbeat.ping_frames
                                    }
                                }
                            }
                        ]
                    }
                }
            }),
        );
        self.send_r2_msg(event, &msg.ws_id);
    }
}

/// Finish the setup flow.
///
/// For a successful setup flow, a new connection to HA is started with the new settings.  
/// This triggers the setup flow change event with the setup state.  
impl Handler<FinishSetupFlowMsg> for Controller {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: FinishSetupFlowMsg, ctx: &mut Self::Context) -> Self::Result {
        let input = if msg.error.is_none() {
            Successful
        } else {
            SetupError
        };
        if self.sm_consume(&msg.ws_id, &input, ctx).is_err() {
            return Box::pin(fut::ready(()));
        }

        if let Some(session) = self.sessions.get_mut(&msg.ws_id) {
            session.reconfiguring = None;
        };

        let mut delay = None;
        if matches!(self.machine.state(), &OperationModeState::Running) {
            info!("Reconnecting to HA with new configuration settings");
            ctx.notify(ConnectMsg::default());
            // delay to notify R2 that the setup flow is finished
            delay = Some(Duration::from_secs(2));
        }

        let state = if msg.error.is_none() {
            IntegrationSetupState::Ok
        } else {
            IntegrationSetupState::Error
        };
        let event = WsMessage::event(
            "driver_setup_change",
            EventCategory::Device,
            serde_json::to_value(DriverSetupChange {
                event_type: SetupChangeEventType::Stop,
                state: state.clone(),
                error: msg.error,
                require_user_action: None,
            })
            .expect("DriverSetupChange serialize error"),
        );

        Box::pin(
            async move {
                // quick and dirty wait for the client connection to be most likely connected
                if let Some(delay) = delay {
                    sleep(delay).await;
                }
            }
            .into_actor(self) // converts future to ActorFuture
            .map(move |_, act, _ctx| {
                info!("Setup flow finished: sending driver_setup_change STOP with state {state}");
                act.send_r2_msg(event, &msg.ws_id);
            }),
        )
    }
}

impl Handler<AbortDriverSetup> for Controller {
    type Result = ();

    fn handle(&mut self, msg: AbortDriverSetup, ctx: &mut Self::Context) -> Self::Result {
        debug!(
            "[{}] abort driver setup request, timeout: {}",
            msg.ws_id, msg.timeout
        );

        if msg.timeout {
            if self.sm_consume(&msg.ws_id, &SetupError, ctx).is_err() {
                return;
            }
            // notify Remote Two that we ran into a timeout
            ctx.notify(FinishSetupFlowMsg {
                ws_id: msg.ws_id,
                error: Some(IntegrationSetupError::Timeout),
            })
        } else {
            // abort: Remote Two aborted setup flow
            if self.sm_consume(&msg.ws_id, &AbortSetup, ctx).is_err() {
                return;
            }

            // Continue normal operation if it was a reconfiguration and not an initial setup.
            // Otherwise we'll always get a "setup required" when requesting entities in the web-configurator.
            if let Some(session) = self.sessions.get_mut(&msg.ws_id) {
                let reconfiguring = session.reconfiguring;
                session.reconfiguring = None;
                if matches!(self.machine.state(), &OperationModeState::RequireSetup)
                    && reconfiguring == Some(true)
                    && self.settings.hass.url.has_host()
                    && !self.settings.hass.token.is_empty()
                {
                    let _ = self.sm_consume(&msg.ws_id, &ConfigurationAvailable, ctx);
                    ctx.notify(ConnectMsg::default());
                }
            }
        }

        if let Some(handle) = self.setup_timeout.take() {
            ctx.cancel_future(handle);
        }

        // Note: this is the place to cleanup any setup activities
        // e.g. stopping the planned Home Assistant mDNS server discovery etc
        // For now it's just a state transition
    }
}

fn parse_value<T: FromStr>(map: &HashMap<String, String>, key: &str) -> Option<T> {
    map.get(key).and_then(|v| T::from_str(v).ok())
}

/// Validate and convert Home Assistant WebSocket URL
fn validate_url<'a>(addr: impl Into<Option<&'a str>>) -> Result<Url, ServiceError> {
    let addr = match addr.into() {
        None => return Err(BadRequest("Missing field: url".into())),
        Some(addr) => addr.trim(),
    };

    // user provided URL might missing scheme
    let mut url = match Url::parse(addr) {
        Ok(url) => url,
        Err(url::ParseError::RelativeUrlWithoutBase) => parse_with_ws_scheme(addr)?,
        Err(e) => {
            warn!("Invalid WebSocket URL '{addr}': {e}");
            return Err(e.into());
        }
    };

    // quirk of URL parsing: hostname:port detects the hostname as scheme!
    if url.host_str().is_none() {
        url = parse_with_ws_scheme(addr)?;
    }

    match url.scheme() {
        "http" => {
            let _ = url.set_scheme("ws");
        }
        "https" => {
            let _ = url.set_scheme("wss");
        }
        "ws" | "wss" => { /* ok */ }
        _ => {
            return Err(BadRequest(
                "Invalid scheme, allowed: ws, wss, http, https".into(),
            ))
        }
    }

    Ok(url)
}

fn parse_with_ws_scheme(address: &str) -> Result<Url, url::ParseError> {
    let address = format!("ws://{address}");
    Url::parse(&address).map_err(|e| {
        warn!("Invalid URL '{address}': {e}");
        e
    })
}

#[cfg(test)]
mod tests {
    use super::validate_url;
    use crate::errors::{ServiceError, ServiceError::BadRequest};
    use url::Url;

    fn url(url: &str) -> Result<Url, ServiceError> {
        match Url::parse(url) {
            Ok(url) => Ok(url),
            Err(e) => panic!("valid URL required! {e}"),
        }
    }

    #[test]
    fn empty_address_returns_error() {
        let result = validate_url(None);
        assert!(matches!(result, Err(BadRequest(_))));
        let result = validate_url("");
        assert!(matches!(result, Err(BadRequest(_))));
        let result = validate_url("  ");
        assert!(matches!(result, Err(BadRequest(_))));
    }

    #[test]
    fn host_only() {
        assert_eq!(url("ws://test/"), validate_url("test"));
    }

    #[test]
    fn valid_address_returns_url() {
        assert_eq!(
            url("ws://homeassistant.local:8123/api/websocket"),
            validate_url("ws://homeassistant.local:8123/api/websocket")
        );
    }

    #[test]
    fn address_with_spaces_are_trimmed() {
        assert_eq!(url("ws://test/"), validate_url("  test   "));
        assert_eq!(
            url("ws://homeassistant.local:8123/api/websocket"),
            validate_url("  ws://homeassistant.local:8123/api/websocket   ")
        );
    }

    #[test]
    fn host_only_with_port() {
        assert_eq!(url("ws://test:8123/"), validate_url("test:8123"));
    }

    #[test]
    fn ip_address_only() {
        assert_eq!(url("ws://127.0.0.1/"), validate_url("127.0.0.1"));
    }

    #[test]
    fn ip_address_only_with_port() {
        assert_eq!(url("ws://127.0.0.1:123/"), validate_url("127.0.0.1:123"));
    }

    #[test]
    fn add_scheme_if_missing() {
        assert_eq!(url("ws://test:123/foo"), validate_url("test:123/foo"));
    }

    #[test]
    fn force_ws_scheme_from_http() {
        assert_eq!(url("ws://test/"), validate_url("http://test"));
        assert_eq!(url("wss://test/"), validate_url("https://test"));
        assert_eq!(url("ws://test/"), validate_url("HTTP://test"));
        assert_eq!(url("wss://test/"), validate_url("HTTPS://test"));
    }

    #[test]
    fn invalid_scheme_returns_error() {
        let result = validate_url("foo://test");
        assert!(matches!(result, Err(BadRequest(_))));
    }
}
