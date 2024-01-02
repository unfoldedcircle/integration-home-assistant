// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for Home Assistant client connection messages.

use crate::client::messages::{Close, ConnectionEvent, ConnectionState};
use crate::client::HomeAssistantClient;
use crate::controller::handler::{ConnectMsg, DisconnectMsg};
use crate::controller::{Controller, OperationModeState};
use actix::{fut, ActorFutureExt, AsyncContext, Context, Handler, ResponseActFuture, WrapFuture};
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
        if !matches!(self.machine.state(), &OperationModeState::Running) {
            error!("Cannot connect in state: {:?}", self.machine.state());
            return Box::pin(fut::result(Err(Error::new(
                ErrorKind::InvalidInput,
                "Not in running state",
            ))));
        }

        if let Some(client_id) = self.ha_client_id.as_ref() {
            warn!("[{client_id}] Ignoring connect request: already connected to HA server");
            return Box::pin(fut::ok(()));
        }

        self.set_device_state(DeviceState::Connecting);

        let ws_request = self.ws_client.ws(self.settings.hass.url.as_str());
        // align frame size to Home Assistant
        let ws_request = ws_request.max_frame_size(self.settings.hass.max_frame_size_kb * 1024);
        let url = self.settings.hass.url.clone();
        let token = self.settings.hass.token.clone();
        let client_address = ctx.address();
        let heartbeat = self.settings.hass.heartbeat;

        Box::pin(
            async move {
                debug!("Connecting to: {url}");

                let (_, framed) = match ws_request.connect().await {
                    Ok((r, f)) => (r, f),
                    Err(e) => {
                        warn!("Could not connect to {url}: {e:?}");
                        return Err(Error::new(ErrorKind::Other, e.to_string()));
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
                match result {
                    Ok(addr) => {
                        debug!("Successfully connected to: {}", act.settings.hass.url);
                        act.ha_client = Some(addr);
                        act.ha_reconnect_duration = act.settings.hass.reconnect.duration;
                        act.ha_reconnect_attempt = 0;
                        Ok(())
                    }
                    Err(e) => {
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
