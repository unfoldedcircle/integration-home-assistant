// Copyright (c) 2026 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Bridge between the WebSocket client actor and the [`HomeAssistantClient`] actor.
//!
//! The WebSocket client owns the connection: connecting, reconnecting with backoff and the
//! optional ping frame keep-alive. Its callbacks are forwarded as [`WsEvent`] actor messages, in
//! order, to the Home Assistant client which implements the Home Assistant protocol on top.

use crate::client::HomeAssistantClient;
use crate::client::messages::{Close, ConnectionEvent, ConnectionState};
use actix::{ActorContext, AsyncContext, Context, Handler, Message, WeakAddr};
use actix_ws_client::{DisconnectReason, Error, HandlerAction, WebSocketMessageHandler};
use bytes::Bytes;
use log::{debug, info, warn};
use std::time::Instant;

/// Event of the WebSocket connection to Home Assistant.
#[derive(Message)]
#[rtype(result = "()")]
pub(super) enum WsEvent {
    /// A new connection is established.
    Connected,
    /// Text message received.
    Text(String),
    /// Binary message received.
    Binary(Bytes),
    /// The connection ended. The WebSocket client reconnects, unless it was closed for good.
    Closed(DisconnectReason),
    /// A connection attempt or the connection failed.
    Error(Error),
    /// The configured number of reconnect attempts is reached: the WebSocket client gave up.
    ReconnectExhausted,
}

/// Forwards the WebSocket client callbacks to the [`HomeAssistantClient`] actor.
///
/// Holds a weak address only: the Home Assistant client owns the WebSocket client, not the other
/// way around.
pub(super) struct WsBridge {
    client: WeakAddr<HomeAssistantClient>,
}

impl WsBridge {
    pub(super) fn new(client: WeakAddr<HomeAssistantClient>) -> Self {
        Self { client }
    }

    fn forward(&self, event: WsEvent) {
        if let Some(client) = self.client.upgrade() {
            // do_send: connection events must not be dropped on a full mailbox
            client.do_send(event);
        }
    }
}

impl WebSocketMessageHandler for WsBridge {
    fn on_text(&mut self, text: String) -> Vec<HandlerAction> {
        self.forward(WsEvent::Text(text));
        Vec::new()
    }

    fn on_binary(&mut self, data: Bytes) -> Vec<HandlerAction> {
        self.forward(WsEvent::Binary(data));
        Vec::new()
    }

    fn on_connected(&mut self) -> Vec<HandlerAction> {
        self.forward(WsEvent::Connected);
        Vec::new()
    }

    fn on_closed(&mut self, reason: &DisconnectReason) -> Vec<HandlerAction> {
        self.forward(WsEvent::Closed(reason.clone()));
        Vec::new()
    }

    fn on_error(&mut self, error: &Error) -> Vec<HandlerAction> {
        self.forward(WsEvent::Error(error.clone()));
        Vec::new()
    }

    fn on_reconnect_exhausted(&mut self) -> Vec<HandlerAction> {
        self.forward(WsEvent::ReconnectExhausted);
        Vec::new()
    }
}

impl Handler<WsEvent> for HomeAssistantClient {
    type Result = ();

    fn handle(&mut self, msg: WsEvent, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            WsEvent::Connected => {
                info!(
                    "[{}] Connected to: {} ({})",
                    self.id, self.url, self.heartbeat
                );
                // every connection starts a new Home Assistant session
                self.reset_session(ctx);
                self.connected = true;
                self.last_hb = Instant::now();
                self.heartbeat(ctx);
                self.controller_actor.do_send(ConnectionEvent {
                    client_id: self.id.clone(),
                    state: ConnectionState::Opened,
                });
            }
            WsEvent::Text(text) => {
                // a queued message of a connection which is already closed
                if self.connected {
                    self.on_text_message(Bytes::from(text), ctx);
                }
            }
            WsEvent::Binary(data) => {
                if self.connected {
                    self.on_binary_message(data, ctx);
                }
            }
            WsEvent::Closed(reason) => {
                match &reason {
                    DisconnectReason::ClosedByServer { code, reason } => info!(
                        "[{}] HA closed connection. Code: {code:?}, reason: {reason}",
                        self.id
                    ),
                    reason => info!("[{}] HA connection closed: {reason:?}", self.id),
                }
                self.connected = false;
                self.reset_session(ctx);
                self.controller_actor.do_send(ConnectionEvent {
                    client_id: self.id.clone(),
                    state: ConnectionState::Disconnected,
                });
            }
            WsEvent::Error(e) => {
                // connection attempts are retried by the WebSocket client
                warn!("[{}] HA connection error: {e}", self.id);
            }
            WsEvent::ReconnectExhausted => {
                info!(
                    "[{}] Max reconnect attempts reached ({}). Giving up!",
                    self.id, self.max_reconnect_attempts
                );
                ctx.stop();
            }
        }
    }
}

impl HomeAssistantClient {
    /// Reset everything that belongs to a single Home Assistant connection.
    ///
    /// Request ids, the authentication, event subscriptions, pending requests and assist sessions
    /// are only valid for the connection they were created on. Settings from the Remote
    /// (subscribed entities, remote identifier, language) are kept.
    fn reset_session(&mut self, ctx: &mut Context<HomeAssistantClient>) {
        if let Some(handle) = self.heartbeat_handle.take() {
            ctx.cancel_future(handle);
        }
        if let Some(handle) = self.uc_ha_comp_check_handle.take() {
            ctx.cancel_future(handle);
        }
        self.ws_id = 0;
        self.authenticated = false;
        self.uc_ha_component = false;
        self.uc_ha_component_info_id = None;
        self.subscribed_events = false;
        self.subscribe_standard_events_id = None;
        self.subscribe_uc_events_id = None;
        self.subscribe_configure_id = None;
        self.entity_states_id = None;
        if !self.open_requests.is_empty() {
            debug!(
                "[{}] Dropping {} pending request(s)",
                self.id,
                self.open_requests.len()
            );
        }
        // dropping the response channels notifies the waiting callers
        self.open_requests.clear();
        self.assist_sessions.clear();
    }

    /// Handle a [`Close`] request. See [`Close`] for the two modes.
    pub(super) fn close(&mut self, msg: Close, ctx: &mut Context<HomeAssistantClient>) {
        let close = actix_ws_client::Close::new(msg.code, msg.description.unwrap_or_default());
        if msg.reconnect {
            info!(
                "[{}] Closing HomeAssistant connection (code {}), reconnecting",
                self.id, msg.code
            );
            // Stop using the broken session right away: the close confirmation follows.
            self.connected = false;
            self.authenticated = false;
            self.ws.do_send(close.with_reconnect());
        } else {
            info!("[{}] Close msg: sending Close to HomeAssistant", self.id);
            self.connected = false;
            self.authenticated = false;
            // The WebSocket client sends the Close frame, stops reconnecting and tears down the
            // connection, also if the server doesn't answer.
            self.ws.do_send(close);
            ctx.stop();
        }
    }
}
