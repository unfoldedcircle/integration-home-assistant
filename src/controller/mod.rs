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
use uc_api::intg::{DeviceState, IntegrationDriverUpdate};
use uc_api::ws::{EventCategory, WsMessage};

state_machine! {
    derive(Debug)
    OperationMode(RequireSetup)

    RequireSetup => {
        ConfigurationAvailable => Running,
        AbortSetup => RequireSetup,
        SetupDriverRequest => SetupFlow [SetupFlowTimer],
    },
    Running(SetupDriverRequest) => SetupFlow [SetupFlowTimer],
    Running(R2Request) => Running,
    SetupFlow => {
        RequestUserInput => WaitSetupUserData,
        Successful => Running [CancelSetupFlowTimer],
        SetupError => SetupError [CancelSetupFlowTimer],
        AbortSetup => RequireSetup [CancelSetupFlowTimer],
    },
    WaitSetupUserData => {
        SetupUserData => SetupFlow,
        SetupError => SetupError [CancelSetupFlowTimer],
        AbortSetup => RequireSetup [CancelSetupFlowTimer],
    },
    SetupError => {
        AbortSetup => RequireSetup,
        SetupDriverRequest => SetupFlow,
        SetupError => SetupError,
    }
}

struct R2Session {
    recipient: Recipient<SendWsMessage>,
    standby: bool,
    subscribed_entities: HashSet<String>,
    /// HomeAssistant connection mode: true = connect (& reconnect), false = disconnect (& don't reconnect)
    ha_connect: bool,
    // TODO replace with request id map & oneshot notification
    /// quick and dirty request id mapping for get_available_entities request.
    get_available_entities_id: Option<u32>,
    /// quick and dirty request id mapping for get_entity_states request.
    get_entity_states_id: Option<u32>,
}

impl R2Session {
    fn new(recipient: Recipient<SendWsMessage>) -> Self {
        Self {
            recipient,
            standby: false,
            subscribed_entities: Default::default(),
            ha_connect: false,
            get_available_entities_id: None,
            get_entity_states_id: None,
        }
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
    ha_reconnect_duration: Duration,
    ha_reconnect_attempt: u32,
    drv_metadata: IntegrationDriverUpdate,
    /// State machine for driver state: setup flow or running state
    machine: StateMachine<OperationMode>,
    /// Driver setup timeout handle
    setup_timeout: Option<SpawnHandle>,
}

impl Controller {
    pub fn new(settings: Settings, drv_metadata: IntegrationDriverUpdate) -> Self {
        let mut machine = StateMachine::new();
        // if we have all required HA connection settings, we can skip driver setup
        if settings.hass.url.has_host() && !settings.hass.token.is_empty() {
            let _ = machine.consume(&OperationModeInput::ConfigurationAvailable);
        } else {
            info!("Home Assistant connection requires setup");
        }
        Self {
            sessions: Default::default(),
            device_state: DeviceState::Disconnected,
            ws_client: new_websocket_client(
                Duration::from_secs(settings.hass.connection_timeout as u64),
                matches!(settings.hass.url.scheme(), "wss" | "https"),
            ),
            ha_reconnect_duration: settings.hass.reconnect.duration,
            settings,
            ha_client: None,
            ha_reconnect_attempt: 0,
            drv_metadata,
            machine,
            setup_timeout: None,
        }
    }

    /// Send a WebSocket message to the remote
    fn send_r2_msg(&self, message: WsMessage, ws_id: &str) {
        if let Some(session) = self.sessions.get(ws_id) {
            if session.standby {
                debug!("Remote is in standby, not sending message: {:?}", message);
                // TODO queue entity update events? #5
                return;
            }
            if let Err(e) = session.recipient.try_send(SendWsMessage(message)) {
                error!("{ws_id} Internal message send error: {e}");
            }
        } else {
            warn!("attempting to send message but couldn't find session: {ws_id}");
        }
    }

    fn send_device_state(&self, ws_id: &str) {
        self.send_r2_msg(
            WsMessage::event(
                "device_state",
                EventCategory::Device,
                json!({ "state": self.device_state }),
            ),
            ws_id,
        );
    }

    fn broadcast_device_state(&self) {
        for session in self.sessions.keys() {
            // TODO filter out remotes which don't require an active HA connection?
            self.send_device_state(session);
        }
    }

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
        debug!(
            "State machine input: {input:?}, state: {:?}",
            self.machine.state()
        );
        match self.machine.consume(input) {
            Ok(None) => {
                info!("State machine transition: {:?}", self.machine.state());
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
