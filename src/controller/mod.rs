// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Central controller handling integration WS requests and HA client connection.

mod handler;
mod messages;

pub use messages::*;

use crate::client::HomeAssistantClient;
use crate::configuration::Settings;
use crate::util::new_websocket_client;
use actix::prelude::{Actor, Context, Recipient};
use actix::Addr;
use log::{debug, error, info, warn};
use rust_fsm::*;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use uc_api::intg::{DeviceState, IntegrationDriverUpdate};
use uc_api::ws::{EventCategory, WsMessage};

state_machine! {
    derive(Debug)
    OperationMode(RequireSetup)

    RequireSetup => {
        ConfigurationAvailable => Running,
        SetupDriverRequest => SetupFlow [SetupFlowTimer],
    },
    Running(SetupDriverRequest) => SetupFlow [SetupFlowTimer],
    Running(R2Request) => Running,
    SetupFlow => {
        AbortSetup => RequireSetup,
        // Successful => Running,
        RequestUserInput => WaitSetupUserData
    },
    WaitSetupUserData => {
        // SetupUserRequestTimeout => SetupError,
        SetupUserData => SetupFlow,
    },
    // SetupError => {
    //     AbortSetup => RequireSetup,
    //     SetupDriverRequest => SetupFlow,
    // }
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

pub struct Controller {
    // TODO use actor address instead? Recipient is generic but only allows one specific message
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
    ha_reconnect_attempt: u16,
    drv_metadata: IntegrationDriverUpdate,
    machine: StateMachine<OperationMode>,
}

impl Controller {
    pub fn new(settings: Settings, drv_metadata: IntegrationDriverUpdate) -> Self {
        let mut machine = StateMachine::new();
        // first baby step, configuration is still read from yaml file
        let _ = machine.consume(&OperationModeInput::ConfigurationAvailable);
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
}

impl Actor for Controller {
    type Context = Context<Self>;
}
