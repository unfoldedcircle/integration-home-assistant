// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for Home Assistant client connection messages.

use crate::client::HomeAssistantClient;
use crate::client::messages::{
    Close, ConnectionEvent, ConnectionState, SetLanguage, SetRemoteId, SubscribedEntities,
};
use crate::controller::handler::{ConnectMsg, DisconnectMsg};
use crate::controller::operation_mode::Input::{AbortSetup, Connected};
use crate::controller::{Controller, operation_mode};
use actix::{AsyncContext, Context, Handler};
use log::{debug, error, info};
use uc_api::intg::DeviceState;

impl Handler<ConnectionEvent> for Controller {
    type Result = ();

    fn handle(&mut self, msg: ConnectionEvent, ctx: &mut Self::Context) -> Self::Result {
        // Only events from the current client are relevant. `ha_client_id` is set as soon as the
        // client actor is created, so this also covers clients which never reached `Connected`.
        if Some(&msg.client_id) != self.ha_client_id.as_ref() {
            info!("[{}] Ignoring event from old HA client", msg.client_id);
            return;
        }

        match msg.state {
            ConnectionState::Opened => {
                let dummy_ws_id = "0"; // we don't have a WS request msg id
                if let Err(e) = self.sm_consume(dummy_ws_id, &Connected, ctx) {
                    error!("{e}");
                }
            }
            ConnectionState::AuthenticationFailed => {
                // The client doesn't reconnect with an invalid token: it stops and sends a
                // Closed event.
                self.set_device_state(DeviceState::Error);
            }
            ConnectionState::Connected => {
                // fully connected (authenticated & subscribed)
                self.set_device_state(DeviceState::Connected);
            }
            ConnectionState::Disconnected => {
                // the client reconnects on its own
                if self.device_state == DeviceState::Connected {
                    info!("[{}] HA connection lost, reconnecting", msg.client_id);
                    self.set_device_state(DeviceState::Connecting);
                }
            }
            ConnectionState::Closed => {
                info!("[{}] HA client disconnected", msg.client_id);
                self.ha_client = None;
                self.ha_client_id = None;

                // The client only stops on its own if it gave up reconnecting. A new connect
                // request from the Remote starts a new client.
                if matches!(
                    self.device_state,
                    DeviceState::Connecting | DeviceState::Connected
                ) {
                    self.set_device_state(DeviceState::Error);
                }
            }
        };
    }
}

impl Handler<DisconnectMsg> for Controller {
    type Result = ();

    fn handle(&mut self, _msg: DisconnectMsg, ctx: &mut Self::Context) -> Self::Result {
        info!("Disconnect request: forcing immediate disconnect from HA server");
        self.disconnect(ctx)
    }
}

impl Controller {
    pub(crate) fn disconnect(&mut self, _ctx: &mut Context<Controller>) {
        // an explicit disconnect is not an error
        self.set_device_state(DeviceState::Disconnected);

        // the client stops reconnecting, closes the connection and stops
        if let Some(addr) = self.ha_client.take() {
            addr.do_send(Close::default());
        }
        // Make sure the old client is no longer used: its remaining events are ignored
        self.ha_client_id = None;
    }
}

impl Handler<ConnectMsg> for Controller {
    type Result = ();

    fn handle(&mut self, _msg: ConnectMsg, ctx: &mut Self::Context) -> Self::Result {
        if !matches!(
            self.machine.state(),
            &operation_mode::State::Running | &operation_mode::State::RequireSetup
        ) {
            error!("Cannot connect in state: {:?}", self.machine.state());
            return;
        }

        // One client at a time. An existing client is connecting, authenticating, connected or
        // reconnecting: it reports the outcome with a connection event.
        if let Some(client_id) = self.ha_client_id.as_ref() {
            debug!("[{client_id}] Ignoring connect request: HA client already exists");
            return;
        }

        let url = self.settings.hass.get_url();
        let token = self.settings.hass.get_token();

        if url.host_str().is_none() || token.is_empty() {
            error!("Cannot connect: HA url or token missing");
            let dummy_ws_id = "0"; // we don't have a WS request msg id
            if let Err(e) = self.sm_consume(dummy_ws_id, &AbortSetup, ctx) {
                error!("{e}");
            }
            return;
        }

        let client_id = HomeAssistantClient::new_client_id(&url);
        info!(
            "[{client_id}] Connecting to: {url} (timeout: {}s, request_timeout: {}s)",
            self.settings.hass.connection_timeout, self.settings.hass.request_timeout
        );
        let ha_client = match HomeAssistantClient::start(
            client_id.clone(),
            url,
            ctx.address(),
            token,
            &self.settings.hass,
        ) {
            Ok(client) => client,
            Err(e) => {
                error!("[{client_id}] Cannot connect to HA: {e}");
                self.set_device_state(DeviceState::Error);
                return;
            }
        };
        self.set_device_state(DeviceState::Connecting);

        // The client keeps these settings over reconnects and uses them after authentication.
        if let Some(language) = self.language.clone()
            && let Err(e) = ha_client.try_send(SetLanguage { language })
        {
            error!("Error sending language to client: {e:?}");
        }
        debug!("Sending subscribed entities to client for events subscriptions");
        if let Some(session) = self.sessions.values().next() {
            let entities = session.subscribed_entities.clone();
            let remote_id = self.remote_id.clone();
            if let Err(e) = ha_client.try_send(SetRemoteId { remote_id }) {
                error!("Error sending remote identifier to client: {:?}", e);
            }

            if let Err(e) = ha_client.try_send(SubscribedEntities {
                entity_ids: entities,
            }) {
                error!("Error updating subscribed entities to client: {:?}", e);
            }
        }

        self.ha_client_id = Some(client_id);
        self.ha_client = Some(ha_client);
    }
}
