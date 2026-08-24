// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for Home Assistant client connection messages.

use crate::client::HomeAssistantClient;
use crate::client::messages::{
    Close, ConnectionEvent, ConnectionState, SetRemoteId, SubscribedEntities,
};
use crate::controller::OperationModeInput::{AbortSetup, Connected};
use crate::controller::connection_state::ConnectionAction;
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
        let accepted = match msg.state {
            ConnectionState::Closed => self.ha_connection.accepts_closed_event(msg.attempt),
            ConnectionState::AuthenticationFailed | ConnectionState::Connected => {
                self.ha_connection.accepts_active_event(msg.attempt)
            }
        };
        if !accepted {
            info!(
                "[{}] Ignoring stale/inactive HA client event for connection attempt {}",
                msg.client_id, msg.attempt
            );
            return;
        }

        match msg.state {
            ConnectionState::AuthenticationFailed => {
                // Authentication errors are terminal for this configuration: a bad token
                // cannot self-heal. An explicit reconfiguration/connect starts a new attempt.
                self.set_device_state(DeviceState::Error);
                self.ha_connection.disconnect();
                self.close_ha_client();
            }
            ConnectionState::Connected => {
                if !self.ha_connection.client_active(msg.attempt) {
                    return;
                }
                self.ha_client_id = Some(msg.client_id);
                self.set_device_state(DeviceState::Connected);
            }
            ConnectionState::Closed => {
                // `accepts_closed_event` above is the primary stale-event filter. Keep
                // client_closed defensive so future direct callers cannot revive an old attempt.
                debug_assert!(self.ha_connection.accepts_closed_event(msg.attempt));
                let action = self.ha_connection.client_closed(msg.attempt);
                info!("[{}] HA client disconnected", msg.client_id);
                self.ha_client = None;
                self.ha_client_id = None;
                self.handle_connection_action(action, ctx, &msg.client_id);
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
        self.set_device_state(DeviceState::Disconnected);

        if let Some(handle) = self.reconnect_handle.take() {
            ctx.cancel_future(handle);
        }
        self.ha_connection.disconnect();
        self.close_ha_client();
    }

    fn close_ha_client(&mut self) {
        self.ha_client_id = None;
        if let Some(addr) = self.ha_client.take() {
            addr.do_send(Close::default());
        }
    }

    fn handle_connection_action(
        &mut self,
        action: ConnectionAction,
        ctx: &mut Context<Controller>,
        client_id: &str,
    ) {
        match action {
            ConnectionAction::ConnectImmediately => {
                info!("[{client_id}] Starting queued HA connection");
                ctx.notify(ConnectMsg::default());
            }
            ConnectionAction::RetryAfterBackoff => {
                info!("[{client_id}] Scheduling HA reconnect");
                self.set_device_state(DeviceState::Connecting);
                self.reconnect_handle =
                    Some(ctx.notify_later(ConnectMsg::default(), self.ha_reconnect_duration));
            }
            ConnectionAction::Stop => {}
            ConnectionAction::IgnoreStale => {
                debug!("[{client_id}] Ignoring stale HA connection completion");
            }
        }
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

        let Some(attempt) = self.ha_connection.begin_connect() else {
            if self.ha_connection.queue_connect_when_closed() {
                info!("Queueing HA connection until the old client has stopped");
            } else {
                debug!("Ignoring HA connect request: a connection is already active or pending");
            }
            return Box::pin(fut::ok(()));
        };

        let url = self.settings.hass.get_url();
        let token = self.settings.hass.get_token();

        if url.host_str().is_none() || token.is_empty() {
            self.ha_connection.stop();
            self.ha_connection.connection_failed(attempt);
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
                // AWC bounds DNS/TCP/TLS with Connector::timeout(connection_timeout)
                // and the WebSocket upgrade request with ClientBuilder::timeout(request_timeout).
                // Any timeout is returned here and follows the normal backoff lifecycle below.
                let (_, framed) = match ws_request.connect().await {
                    Ok((r, f)) => (r, f),
                    Err(e) => {
                        warn!("Could not connect to {url}: {e:?}");
                        return Err(Error::other(e.to_string()));
                    }
                };
                info!("Connected to: {url} ({heartbeat})");

                let (sink, stream) = framed.split();
                let addr = HomeAssistantClient::start(
                    url,
                    client_address,
                    token,
                    sink,
                    stream,
                    heartbeat,
                    attempt,
                );

                Ok(addr)
            }
            .into_actor(self)
            .map(move |result, act, ctx| match result {
                Ok(addr) => {
                    if !act.ha_connection.client_started(attempt) {
                        info!("Discarding stale HA connection attempt {attempt}");
                        addr.do_send(Close::default());
                        return Ok(());
                    }

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
                    let action = act.ha_connection.connection_failed(attempt);
                    match action {
                        ConnectionAction::RetryAfterBackoff => {
                            act.ha_reconnect_attempt += 1;
                            if act.settings.hass.reconnect.attempts > 0
                                && act.ha_reconnect_attempt > act.settings.hass.reconnect.attempts
                            {
                                info!(
                                    "Max reconnect attempts reached ({}). Giving up!",
                                    act.settings.hass.reconnect.attempts
                                );
                                act.ha_connection.stop();
                                act.set_device_state(DeviceState::Error);
                            } else {
                                act.handle_connection_action(action, ctx, "connect-failed");
                                act.increment_reconnect_timeout();
                            }
                        }
                        ConnectionAction::ConnectImmediately
                        | ConnectionAction::Stop
                        | ConnectionAction::IgnoreStale => {
                            act.handle_connection_action(action, ctx, "connect-failed");
                        }
                    }
                    Err(e)
                }
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    // These controller-message tests cover event ordering without a live HA socket.
    // As HomeAssistantClient::start currently requires an awc Framed socket and has no
    // injectable factory, exact actor-count verification remains deterministic at the
    // ConnectionLifecycle level rather than in this controller harness.
    use super::*;
    use crate::configuration::Settings;
    use actix::{Actor, Message, MessageResult};

    #[derive(Debug)]
    struct ReplacementAttempts {
        first: u64,
        second: u64,
    }

    #[derive(Debug)]
    struct ControllerSnapshot {
        usable: bool,
        device_state: String,
    }

    #[derive(Message)]
    #[rtype(result = "ReplacementAttempts")]
    struct PrepareReplacement;

    #[derive(Message)]
    #[rtype(result = "u64")]
    struct PrepareConnecting;

    #[derive(Message)]
    #[rtype(result = "bool")]
    struct CanStartClient(u64);

    #[derive(Message)]
    #[rtype(result = "ControllerSnapshot")]
    struct Snapshot;

    impl Handler<PrepareReplacement> for Controller {
        type Result = MessageResult<PrepareReplacement>;

        fn handle(&mut self, _msg: PrepareReplacement, _ctx: &mut Context<Self>) -> Self::Result {
            let first = self.ha_connection.begin_connect().expect("first attempt");
            assert!(self.ha_connection.client_started(first));
            assert!(self.ha_connection.client_active(first));
            self.set_device_state(DeviceState::Connected);
            self.ha_connection.disconnect();
            assert!(self.ha_connection.queue_connect_when_closed());
            assert_eq!(
                self.ha_connection.client_closed(first),
                ConnectionAction::ConnectImmediately
            );
            let second = self
                .ha_connection
                .begin_connect()
                .expect("replacement attempt");
            assert!(self.ha_connection.client_started(second));
            assert!(self.ha_connection.client_active(second));
            self.set_device_state(DeviceState::Connected);
            MessageResult(ReplacementAttempts { first, second })
        }
    }

    impl Handler<PrepareConnecting> for Controller {
        type Result = u64;

        fn handle(&mut self, _msg: PrepareConnecting, _ctx: &mut Context<Self>) -> Self::Result {
            let attempt = self
                .ha_connection
                .begin_connect()
                .expect("connection starts");
            self.set_device_state(DeviceState::Connecting);
            attempt
        }
    }

    impl Handler<CanStartClient> for Controller {
        type Result = bool;

        fn handle(&mut self, msg: CanStartClient, _ctx: &mut Context<Self>) -> Self::Result {
            self.ha_connection.client_started(msg.0)
        }
    }

    impl Handler<Snapshot> for Controller {
        type Result = MessageResult<Snapshot>;

        fn handle(&mut self, _msg: Snapshot, _ctx: &mut Context<Self>) -> Self::Result {
            MessageResult(ControllerSnapshot {
                usable: self.ha_connection.is_usable(),
                device_state: self.device_state.to_string(),
            })
        }
    }

    fn controller() -> actix::Addr<Controller> {
        let metadata = serde_json::from_str("{}").expect("empty driver metadata is valid");
        Controller::new(Settings::default(), metadata).start()
    }

    #[actix::test]
    async fn stale_closed_event_cannot_tear_down_the_replacement_attempt() {
        let controller = controller();
        let attempts = controller.send(PrepareReplacement).await.unwrap();
        assert_ne!(attempts.first, attempts.second);

        controller
            .send(ConnectionEvent {
                client_id: "first".into(),
                attempt: attempts.first,
                state: ConnectionState::Closed,
            })
            .await
            .unwrap();

        let snapshot = controller.send(Snapshot).await.unwrap();
        assert!(snapshot.usable);
        assert_eq!(snapshot.device_state, "CONNECTED");
    }

    #[actix::test]
    async fn authentication_failure_before_client_attachment_cannot_attach_orphan() {
        let controller = controller();
        let attempt = controller.send(PrepareConnecting).await.unwrap();

        controller
            .send(ConnectionEvent {
                client_id: "pending".into(),
                attempt,
                state: ConnectionState::AuthenticationFailed,
            })
            .await
            .unwrap();

        let snapshot = controller.send(Snapshot).await.unwrap();
        assert!(!snapshot.usable);
        assert_eq!(snapshot.device_state, "ERROR");
        assert!(!controller.send(CanStartClient(attempt)).await.unwrap());
    }
}
