// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Central controller handling integration WS requests and HA client connection.

mod handler;
mod messages;

pub use messages::*;

use crate::client::HomeAssistantClient;
use crate::configuration::{Settings, DEF_SETUP_TIMEOUT_SEC, ENV_SETUP_TIMEOUT};
use crate::controller::handler::AbortDriverSetup;
use crate::errors::ServiceError;
use crate::util::new_websocket_client;
use actix::prelude::{Actor, Context, Recipient};
use actix::{Addr, AsyncContext, SpawnHandle};
use log::{debug, error, info, warn};
use rust_fsm::*;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::env;
use std::str::FromStr;
use std::time::Duration;
use uc_api::intg::{AvailableIntgEntity, DeviceState, IntegrationDriverUpdate};
use uc_api::ws::{EventCategory, WsMessage};

state_machine! {
    derive(Debug)
    OperationMode(RequireSetup)

    RequireSetup => {
        ConfigurationAvailable => Running,
        AbortSetup => RequireSetup,
        SetupDriverRequest => SetupFlow [SetupFlowTimer],
        Connected => Running,  // URL & token set with external access token
    },
    Running => {
        SetupDriverRequest => SetupFlow [SetupFlowTimer],
        R2Request => Running,
        Connected => Running,  // reconnection
        AbortSetup => RequireSetup,
    },
    SetupFlow => {
        RequestUserInput => WaitSetupUserData,
        Successful => Running [CancelSetupFlowTimer],
        SetupError => SetupError [CancelSetupFlowTimer],
        AbortSetup => RequireSetup [CancelSetupFlowTimer],
        Connected => SetupFlow,  // setup flow will connect to HA, but final input is Successful
    },
    WaitSetupUserData => {
        SetupUserData => SetupFlow,
        SetupError => SetupError [CancelSetupFlowTimer],
        AbortSetup => RequireSetup [CancelSetupFlowTimer],
        Connected => WaitSetupUserData,
    },
    SetupError => {
        AbortSetup => RequireSetup,
        SetupDriverRequest => SetupFlow,
        SetupError => SetupError,
        Connected => Running,  // should not happen, just for safety
    }
}

struct R2Session {
    recipient: Recipient<SendWsMessage>,
    /// Request message id from driver to remote
    ws_id: u32,
    standby: bool,
    subscribed_entities: HashSet<String>,
    // TODO replace with request id map & oneshot notification
    /// quick and dirty request id mapping for get_available_entities request.
    get_available_entities_id: Option<u32>,
    /// quick and dirty request id mapping for get_entity_states request.
    get_entity_states_id: Option<u32>,
    /// Flag if currently in setup or reconfiguration mode.
    pub reconfiguring: Option<bool>,
}

impl R2Session {
    fn new(recipient: Recipient<SendWsMessage>) -> Self {
        Self {
            recipient,
            ws_id: 0,
            standby: false,
            subscribed_entities: Default::default(),
            get_available_entities_id: None,
            get_entity_states_id: None,
            reconfiguring: None,
        }
    }

    fn new_msg_id(&mut self) -> u32 {
        self.ws_id += 1;
        self.ws_id
    }
}

/// Central controller handling integration WS requests and HA client connection.
///
/// Uses the Actix actor framework to communicate with the Core-Integration server module and
/// the Home Assistant client.  
pub struct Controller {
    /// Active Remote Two WebSocket sessions
    sessions: HashMap<String, R2Session>,
    /// Home Assistant connection state
    device_state: DeviceState,
    settings: Settings,
    /// WebSocket client
    // creating an expensive client is sufficient once per process and can be used to create multiple connections
    ws_client: awc::Client,
    /// HomeAssistant client actor
    ha_client: Option<Addr<HomeAssistantClient>>,
    /// HomeAssistant client identifier
    ha_client_id: Option<String>,
    ha_reconnect_duration: Duration,
    ha_reconnect_attempt: u32,
    drv_metadata: IntegrationDriverUpdate,
    /// State machine for driver state: setup flow or running state
    machine: StateMachine<OperationMode>,
    /// Driver setup timeout handle
    setup_timeout: Option<SpawnHandle>,
    /// Handle to a scheduled connect message for a reconnect attempt.
    reconnect_handle: Option<SpawnHandle>,
    /// List of subscribed entities sent by HA component
    susbcribed_entity_ids: Option<Vec<AvailableIntgEntity>>,
    /// Request id sent to the remote to get the version information
    remote_id: String,
}

impl Controller {
    pub fn new(settings: Settings, drv_metadata: IntegrationDriverUpdate) -> Self {
        let mut machine = StateMachine::new();
        let url = settings.hass.get_url();
        // if we have all required HA connection settings, we can skip driver setup
        if url.has_host() && !settings.hass.get_token().is_empty() {
            let _ = machine.consume(&OperationModeInput::ConfigurationAvailable);
        } else {
            info!("Home Assistant connection requires setup");
        }
        Self {
            sessions: Default::default(),
            device_state: DeviceState::Disconnected,
            ws_client: new_websocket_client(
                Duration::from_secs(settings.hass.connection_timeout as u64),
                Duration::from_secs(settings.hass.request_timeout as u64),
                matches!(url.scheme(), "wss" | "https"),
            ),
            ha_reconnect_duration: settings.hass.reconnect.duration,
            settings,
            ha_client: None,
            ha_client_id: None,
            ha_reconnect_attempt: 0,
            drv_metadata,
            machine,
            setup_timeout: None,
            reconnect_handle: None,
            susbcribed_entity_ids: None,
            remote_id: "".to_string(),
        }
    }

    /// Send a WebSocket message to the remote
    fn send_r2_msg(&self, message: WsMessage, ws_id: &str) {
        if let Some(session) = self.sessions.get(ws_id) {
            if session.standby {
                debug!("Remote is in standby, not sending message: {:?}", message);
                return;
            }
            let msg = message.msg.clone();
            if let Err(e) = session.recipient.try_send(SendWsMessage(message)) {
                error!(
                    "[{ws_id}] Internal message send error of '{}': {e}",
                    msg.unwrap_or_default()
                );
            }
        } else {
            warn!("attempting to send message but couldn't find session: {ws_id}");
        }
    }

    /// Send a `device_state` event message with the current state to the given WebSocket client identifier.
    ///
    /// # Arguments
    ///
    /// * `ws_id`: WebSocket connection identifier of a Remote Two connection.
    ///
    /// returns: ()
    fn send_device_state(&self, ws_id: &str) {
        info!("[{ws_id}] sending device_state: {}", self.device_state);
        self.send_r2_msg(
            WsMessage::event(
                "device_state",
                EventCategory::Device,
                json!({ "state": self.device_state }),
            ),
            ws_id,
        );
    }

    /// Broadcast a `device_state` event message with the current state to all connected Remotes
    fn broadcast_device_state(&self) {
        for session in self.sessions.keys() {
            // TODO filter out remotes which don't require an active HA connection?
            self.send_device_state(session);
        }
    }

    /// Set integration device state and broadcast state to all connected Remotes
    ///
    /// # Arguments
    ///
    /// * `state`: The state to set
    ///
    /// returns: ()
    fn set_device_state(&mut self, state: DeviceState) {
        self.device_state = state;
        self.broadcast_device_state();
    }

    fn increment_reconnect_timeout(&mut self) {
        let new_timeout = Duration::from_millis(
            (self.ha_reconnect_duration.as_millis() as f32
                * self.settings.hass.reconnect.backoff_factor) as u64,
        );

        self.ha_reconnect_duration = if new_timeout.gt(&self.settings.hass.reconnect.duration_max) {
            self.settings.hass.reconnect.duration_max
        } else {
            new_timeout
        };
        info!(
            "New reconnect timeout: {}",
            self.ha_reconnect_duration.as_millis()
        )
    }

    /// Perform a state machine transition for the given input.
    ///
    /// An error is returned, if a state transition with the current state and the provided input
    /// is not allowed.
    fn sm_consume(
        &mut self,
        ws_id: &str,
        input: &OperationModeInput,
        ctx: &mut Context<Controller>,
    ) -> Result<(), ServiceError> {
        let old_state = format!("{:?}", self.machine.state());
        debug!("State machine input: {input:?}, state: {old_state}",);
        match self.machine.consume(input) {
            Ok(None) => {
                let state = format!("{:?}", self.machine.state());
                if state != old_state {
                    info!("State machine transition: {old_state} -> {state}");
                }
                Ok(())
            }
            Ok(Some(OperationModeOutput::SetupFlowTimer)) => {
                if let Some(handle) = self.setup_timeout.take() {
                    ctx.cancel_future(handle);
                }
                let timeout = env::var(ENV_SETUP_TIMEOUT)
                    .ok()
                    .and_then(|v| u64::from_str(&v).ok())
                    .unwrap_or(DEF_SETUP_TIMEOUT_SEC);
                debug!("Starting SetupFlowTimer: {timeout} sec");
                self.setup_timeout = Some(ctx.notify_later(
                    AbortDriverSetup {
                        ws_id: ws_id.to_string(),
                        timeout: true,
                    },
                    Duration::from_secs(timeout),
                ));
                Ok(())
            }
            Ok(Some(OperationModeOutput::CancelSetupFlowTimer)) => {
                debug!("Cancelling SetupFlowTimer");
                if let Some(handle) = self.setup_timeout.take() {
                    ctx.cancel_future(handle);
                }
                Ok(())
            }
            Err(_) => Err(ServiceError::BadRequest(format!(
                "Transition {input:?} not allowed in state {:?}",
                self.machine.state()
            ))),
        }
    }
}

impl Actor for Controller {
    type Context = Context<Self>;
}
