// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for Home Assistant client connection messages.

use crate::client::HomeAssistantClient;
use crate::client::messages::{
    Close, ConnectionEvent, ConnectionState, SetRemoteId, SubscribedEntities,
};
use crate::controller::OperationModeInput::{AbortSetup, Connected};
use crate::controller::handler::{ConnectMsg, DisconnectMsg};
use crate::controller::{Controller, OperationModeState};
use actix::{ActorFutureExt, AsyncContext, Context, Handler, ResponseActFuture, WrapFuture, fut};
use futures::StreamExt;
use log::{debug, error, info, warn};
use std::io::{Error, ErrorKind};
use uc_api::intg::DeviceState;

impl Handler<ConnectionEvent> for Controller {
    type Result = ();

    fn handle(&mut self, msg: ConnectionEvent, ctx: &mut Self::Context) -> Self::Result {
        // TODO #39 state machine with connection & reconnection states (as in remote-core).
        //      This patched-up implementation might still contain race conditions!
        match msg.state {
            ConnectionState::AuthenticationFailed => {
                // error state prevents auto-reconnect in upcoming Closed event
                self.set_device_state(DeviceState::Error);
            }
            ConnectionState::Connected => {
                self.ha_client_id = Some(msg.client_id);
                self.set_device_state(DeviceState::Connected);
            }
            ConnectionState::Closed => {
                if Some(&msg.client_id) == self.ha_client_id.as_ref() {
                    info!("[{}] HA client disconnected", msg.client_id);
                    self.ha_client = None;
                    self.ha_client_id = None;
                } else {
                    info!("[{}] Old HA client disconnected: ignoring", msg.client_id);
                    return;
                }

                if matches!(
                    self.device_state,
                    DeviceState::Connecting | DeviceState::Connected
                ) {
                    info!("[{}] Start reconnecting to HA", msg.client_id);
                    self.set_device_state(DeviceState::Connecting);

                    self.reconnect_handle =
                        Some(ctx.notify_later(ConnectMsg::default(), self.ha_reconnect_duration));
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
    pub(crate) fn disconnect(&mut self, ctx: &mut Context<Controller>) {
        // this prevents automatic reconnects. TODO #39 this should be handled with a state machine!
        self.set_device_state(DeviceState::Disconnected);

        if let Some(handle) = self.reconnect_handle.take() {
            ctx.cancel_future(handle);
        }
        if let Some(addr) = self.ha_client.as_ref() {
            addr.do_send(Close::default());
        }
        // Make sure the old connection is no longer used and doesn't interfere with reconnection
        self.ha_client = None;
        self.ha_client_id = None;
    }
}

impl Handler<ConnectMsg> for Controller {
    type Result = ResponseActFuture<Self, Result<(), Error>>;

    fn handle(&mut self, _msg: ConnectMsg, ctx: &mut Self::Context) -> Self::Result {
        if let Some(handle) = self.reconnect_handle.take() {
            ctx.cancel_future(handle);
        }
        if !matches!(
            self.machine.state(),
            &OperationModeState::Running | &OperationModeState::RequireSetup
        ) {
            error!("Cannot connect in state: {:?}", self.machine.state());
            return Box::pin(fut::result(Err(Error::new(
                ErrorKind::InvalidInput,
                "Not in running state",
            ))));
        }

        if let Some(client_id) = self.ha_client_id.as_ref()
            && self.ha_client.is_some()
        {
            warn!("[{client_id}] Ignoring connect request: already connected to HA server");
            return Box::pin(fut::ok(()));
        }

        let url = self.settings.hass.get_url();
        let token = self.settings.hass.get_token();

        if url.host_str().is_none() || token.is_empty() {
            error!("Cannot connect: HA url or token missing");
            let dummy_ws_id = "0"; // we don't have a WS request msg id
            if let Err(e) = self.sm_consume(dummy_ws_id, &AbortSetup, ctx) {
                error!("{e}");
            }
            return Box::pin(fut::result(Err(Error::new(
                ErrorKind::InvalidInput,
                "Missing HA url or token",
            ))));
        }

        self.set_device_state(DeviceState::Connecting);

        let ws_request = self.ws_client.ws(url.as_str());
        // align frame size to Home Assistant
        let ws_request = ws_request.max_frame_size(self.settings.hass.max_frame_size_kb * 1024);
        let client_address = ctx.address();
        let heartbeat = self.settings.hass.heartbeat;
        let remote_id = self.remote_id.clone();

        info!(
            "Connecting to: {url} (timeout: {}s, request_timeout: {}s)",
            self.settings.hass.connection_timeout, self.settings.hass.request_timeout
        );
        Box::pin(
            async move {
                let (_, framed) = match ws_request.connect().await {
                    Ok((r, f)) => (r, f),
                    Err(e) => {
                        warn!("Could not connect to {url}: {e:?}");
                        return Err(Error::other(e.to_string()));
                    }
                };
                info!("Connected to: {url} ({heartbeat})");

                let (sink, stream) = framed.split();
                let addr =
                    HomeAssistantClient::start(url, client_address, token, sink, stream, heartbeat);

                Ok(addr)
            }
            .into_actor(self) // converts future to ActorFuture
            .map(move |result, act, ctx| {
                act.ha_client_id = None; // will be set with Connected event
                match result {
                    Ok(addr) => {
                        let dummy_ws_id = "0"; // we don't have a WS request msg id
                        if let Err(e) = act.sm_consume(dummy_ws_id, &Connected, ctx) {
                            error!("{e}");
                        }

                        act.ha_client = Some(addr);
                        act.ha_reconnect_duration = act.settings.hass.reconnect.duration;
                        act.ha_reconnect_attempt = 0;
                        debug!("Sending subscribed entities to client for events subscriptions");
                        if let Some(session) = act.sessions.values().next() {
                            let entities = session.subscribed_entities.clone();
                            if let Some(ha_client) = &act.ha_client {
                                if let Err(e) = ha_client.try_send(SetRemoteId { remote_id }) {
                                    error!("Error sending remote identifier to client: {:?}", e);
                                }

                                if let Err(e) = ha_client.try_send(SubscribedEntities {
                                    entity_ids: entities,
                                }) {
                                    error!("Error updating subscribed entities to client: {:?}", e);
                                }
                            }
                        }
                        Ok(())
                    }
                    Err(e) => {
                        act.ha_client = None;
                        // TODO #39 quick and dirty: simply send Connect message as simple reconnect mechanism. Needs to be refined!
                        if act.device_state != DeviceState::Disconnected {
                            act.ha_reconnect_attempt += 1;
                            if act.settings.hass.reconnect.attempts > 0
                                && act.ha_reconnect_attempt > act.settings.hass.reconnect.attempts
                            {
                                info!(
                                    "Max reconnect attempts reached ({}). Giving up!",
                                    act.settings.hass.reconnect.attempts
                                );
                                act.set_device_state(DeviceState::Error);
                            } else {
                                act.reconnect_handle = Some(ctx.notify_later(
                                    ConnectMsg::default(),
                                    act.ha_reconnect_duration,
                                ));
                                act.increment_reconnect_timeout();
                            }
                        }
                        Err(e)
                    }
                }
            }),
        )
    }
}
