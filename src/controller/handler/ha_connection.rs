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
        // Only events from the current client are relevant. `ha_client_id` is set as soon as the
        // client actor is created, so this also covers clients which never reached `Connected`.
        if Some(&msg.client_id) != self.ha_client_id.as_ref() {
            info!("[{}] Ignoring event from old HA client", msg.client_id);
            return;
        }

        match msg.state {
            ConnectionState::AuthenticationFailed => {
                // error state prevents auto-reconnect in upcoming Closed event
                self.set_device_state(DeviceState::Error);
            }
            ConnectionState::Connected => {
                // fully connected (authenticated & subscribed): reset reconnect backoff
                self.ha_reconnect_duration = self.settings.hass.reconnect.duration;
                self.ha_reconnect_attempt = 0;
                self.set_device_state(DeviceState::Connected);
            }
            ConnectionState::Closed => {
                info!("[{}] HA client disconnected", msg.client_id);
                self.ha_client = None;
                self.ha_client_id = None;

                if matches!(
                    self.device_state,
                    DeviceState::Connecting | DeviceState::Connected
                ) {
                    info!("[{}] Start reconnecting to HA", msg.client_id);
                    self.set_device_state(DeviceState::Connecting);

                    self.reconnect_handle =
                        Some(ctx.notify_later(ConnectMsg::default(), self.ha_reconnect_duration));
                    // back off if the server keeps closing the connection before it's usable
                    self.increment_reconnect_timeout();
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
        // this prevents automatic reconnects
        self.set_device_state(DeviceState::Disconnected);

        if let Some(handle) = self.reconnect_handle.take() {
            ctx.cancel_future(handle);
        }
        // invalidate an in-flight connection attempt: its result will be discarded
        self.ha_connect_pending = None;
        if let Some(addr) = self.ha_client.take() {
            addr.do_send(Close::default());
        }
        // Make sure the old connection is no longer used and doesn't interfere with reconnection
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

        // One client at a time: either a client actor exists (connecting, authenticating or
        // connected), or a connection attempt is still in flight. Both will end in a
        // `Connected` or `Closed` event, which drives the next step.
        if let Some(client_id) = self.ha_client_id.as_ref() {
            warn!("[{client_id}] Ignoring connect request: HA client already exists");
            return Box::pin(fut::ok(()));
        }
        if let Some(attempt) = self.ha_connect_pending {
            warn!("Ignoring connect request: connection attempt {attempt} still in progress");
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

        self.ha_connect_seq = self.ha_connect_seq.wrapping_add(1);
        let attempt = self.ha_connect_seq;
        self.ha_connect_pending = Some(attempt);
        let client_id = HomeAssistantClient::new_client_id(&url);

        let ws_request = self.ws_client.ws(url.as_str());
        // align frame size to Home Assistant
        let ws_request = ws_request.max_frame_size(self.settings.hass.max_frame_size_kb * 1024);
        let client_address = ctx.address();
        let heartbeat = self.settings.hass.heartbeat;
        let remote_id = self.remote_id.clone();

        info!(
            "[{client_id}] Connecting to: {url} (timeout: {}s, request_timeout: {}s)",
            self.settings.hass.connection_timeout, self.settings.hass.request_timeout
        );
        Box::pin(
            async move {
                let (_, framed) = match ws_request.connect().await {
                    Ok((r, f)) => (r, f),
                    Err(e) => {
                        warn!("[{client_id}] Could not connect to {url}: {e:?}");
                        return Err(Error::other(e.to_string()));
                    }
                };
                info!("[{client_id}] Connected to: {url} ({heartbeat})");

                let (sink, stream) = framed.split();
                let addr = HomeAssistantClient::start(
                    client_id.clone(),
                    url,
                    client_address,
                    token,
                    sink,
                    stream,
                    heartbeat,
                );

                Ok((addr, client_id))
            }
            .into_actor(self) // converts future to ActorFuture
            .map(move |result, act, ctx| {
                if act.ha_connect_pending != Some(attempt) {
                    // superseded by a disconnect (standby, setup flow) or a newer attempt
                    match result {
                        Ok((addr, client_id)) => {
                            info!("[{client_id}] Discarding superseded HA connection");
                            addr.do_send(Close::default());
                        }
                        Err(e) => debug!("Ignoring failed, superseded connection attempt: {e}"),
                    }
                    return Ok(());
                }
                act.ha_connect_pending = None;

                match result {
                    Ok((addr, client_id)) => {
                        let dummy_ws_id = "0"; // we don't have a WS request msg id
                        if let Err(e) = act.sm_consume(dummy_ws_id, &Connected, ctx) {
                            error!("{e}");
                        }

                        act.ha_client_id = Some(client_id);
                        act.ha_client = Some(addr);
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
