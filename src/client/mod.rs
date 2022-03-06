// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home Assistant client WebSocket API implementation with Actix actors.

use std::time::Instant;

use actix::io::SinkWrite;
use actix::{Actor, ActorContext, Addr, AsyncContext, Context};
use actix_codec::Framed;
use awc::ws::Codec;
use awc::{ws, BoxedSocket};
use bytes::Bytes;
use futures::stream::{SplitSink, SplitStream};
use log::{debug, error, info, warn};
use serde::de::Error;
use serde_json::{json, Value};

use messages::Close;

use crate::client::messages::{ConnectionEvent, ConnectionState};
use crate::client::model::Event;
use crate::configuration::HeartbeatSettings;
use crate::errors::ServiceError;
use crate::Controller;

mod actor;
mod close_handler;
mod event;
mod get_states;
pub mod messages;
mod model;
mod service;
mod streamhandler;

pub struct HomeAssistantClient {
    /// Unique HA client id
    id: String,
    /// HA request message id
    ws_id: u32,
    access_token: String,
    subscribed_events: bool,
    /// request id of the last `subscribe_events` request. This id will be used in the result and event messages.
    subscribe_events_id: Option<u32>,
    /// request id of the last `subscribe_events` request. This id will be used the result message.
    entity_states_id: Option<u32>,
    sink: SinkWrite<ws::Message, SplitSink<Framed<BoxedSocket, Codec>, ws::Message>>,
    // TODO use abstract actix Receiver(s) instead of hard Controller dependency?
    controller_actor: Addr<Controller>,
    /// Last heart beat timestamp.
    last_hb: Instant,
    heartbeat: HeartbeatSettings,
}

impl HomeAssistantClient {
    pub fn start(
        id: String,
        controller_actor: Addr<Controller>,
        access_token: String,
        sink: SplitSink<Framed<BoxedSocket, ws::Codec>, ws::Message>,
        stream: SplitStream<Framed<BoxedSocket, ws::Codec>>,
        heartbeat: HeartbeatSettings,
    ) -> Addr<Self> {
        HomeAssistantClient::create(|ctx| {
            ctx.add_stream(stream);
            HomeAssistantClient {
                id,
                ws_id: 0,
                access_token,
                subscribed_events: false,
                subscribe_events_id: None,
                entity_states_id: None,
                sink: SinkWrite::new(sink, ctx),
                controller_actor,
                last_hb: Instant::now(),
                heartbeat,
            }
        })
    }

    fn new_msg_id(&mut self) -> u32 {
        self.ws_id += 1;
        self.ws_id
    }

    fn heartbeat(&self, ctx: &mut Context<Self>) {
        ctx.run_later(self.heartbeat.interval, |act, ctx| {
            // check server heartbeats
            if Instant::now().duration_since(act.last_hb) > act.heartbeat.timeout {
                // heartbeat timed out
                error!(
                    "[{}] Websocket server heartbeat failed, disconnecting!",
                    act.id
                );

                // Stop sending pings & Stop actor
                ctx.stop();
                return;
            }

            if act
                .send_message(ws::Message::Ping(Bytes::new()), "Ping", ctx)
                .is_ok()
            {
                act.heartbeat(ctx);
            }
        });
    }

    fn on_text_message(&mut self, txt: Bytes, ctx: &mut Context<HomeAssistantClient>) {
        debug!("[{}] -> Text msg: {:?}", self.id, txt);

        let mut msg = match json_object_from_text_msg(&self.id, txt.as_ref()) {
            Ok(m) => m,
            Err(_) => {
                ctx.notify(Close::invalid());
                return;
            }
        };

        let object_msg = msg.as_object_mut().unwrap(); // is_object() checked in json_object_from_text_msg!

        let id = object_msg
            .get("id")
            .and_then(|v| v.as_u64())
            .unwrap_or_default() as u32;
        match object_msg
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
        {
            "event" => {
                // TODO should we only check Event.event_type == "state_changed"? The id check worked well though in YIO v1
                if Some(id) != self.subscribe_events_id {
                    debug!(
                        "[{}] Ignoring event with non matching event subscription id",
                        self.id
                    );
                    return;
                }
                let event = serde_json::from_value::<Event>(
                    object_msg.remove("event").unwrap_or(Value::Null),
                );
                if let Ok(event) = event {
                    if let Err(e) = self.handle_event(event) {
                        error!(
                            "[{}] Error handling HA state_changed event: {:?}",
                            self.id, e
                        );
                    }
                }
            }
            "result" => {
                let success = object_msg
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or_default();
                if Some(id) == self.subscribe_events_id {
                    self.subscribed_events = success;
                    if self.subscribed_events {
                        debug!("[{}] Subscribed to state changes", self.id);
                        self.controller_actor.do_send(ConnectionEvent {
                            client_id: self.id.clone(),
                            state: ConnectionState::Connected,
                        });
                    } else {
                        ctx.notify(Close::invalid());
                    }
                } else if Some(id) == self.entity_states_id {
                    if !success {
                        error!("[{}] get_states request failed", self.id);
                        ctx.notify(Close::invalid());
                    }

                    if let Some(entities) = object_msg.get("result").and_then(|v| v.as_array()) {
                        if let Err(e) = self.handle_get_states_result(entities) {
                            error!("[{}] Error handling HA get_states result: {:?}", self.id, e);
                        }
                    }
                }
            }
            "auth_required" => {
                if let Err(e) = self.send_message(
                    ws::Message::Text(
                        json!({ "type": "auth", "access_token": self.access_token})
                            .to_string()
                            .into(),
                    ),
                    "auth",
                    ctx,
                ) {
                    error!("[{}] Error sending auth to HA: {:?}", self.id, e);
                    ctx.notify(Close::invalid());
                }
            }
            "auth_invalid" => {
                error!("[{}] Invalid authentication", self.id);
                self.controller_actor.do_send(ConnectionEvent {
                    client_id: self.id.clone(),
                    state: ConnectionState::AuthenticationFailed,
                });
            }
            "auth_ok" => {
                info!("[{}] Authentication OK", self.id);

                if !self.subscribed_events {
                    self.subscribe_events_id = Some(self.new_msg_id());
                    if let Err(e) = self.send_message(
                        ws::Message::Text(
                            json!({
                              "id": self.subscribe_events_id.unwrap(),
                              "type": "subscribe_events",
                              "event_type": "state_changed"
                            })
                            .to_string()
                            .into(),
                        ),
                        "subscribe_events",
                        ctx,
                    ) {
                        error!(
                            "[{}] Error sending subscribe_events to HA: {:?}",
                            self.id, e
                        );
                        ctx.notify(Close::invalid());
                    }
                }
            }
            _ => {}
        }
    }

    fn on_binary_message(&mut self, _: Bytes, ctx: &mut Context<HomeAssistantClient>) {
        error!("[{}] Binary messages not supported! Disconnecting", self.id);
        ctx.notify(Close::unsupported());
    }

    fn on_ping_message(&mut self, bytes: Bytes, ctx: &mut Context<HomeAssistantClient>) {
        // HA doesn't seem to initiate pings, but this might change in the future...
        debug!("[{}] -> Ping", self.id);
        self.last_hb = Instant::now();
        let _ = self.send_message(ws::Message::Pong(bytes), "Pong", ctx);
    }

    fn on_pong_message(&mut self, _: Bytes, _: &mut Context<HomeAssistantClient>) {
        debug!("[{}] -> Pong", self.id);
        self.last_hb = Instant::now();
    }

    fn send_message(
        &mut self,
        msg: ws::Message,
        name: &str,
        ctx: &mut Context<HomeAssistantClient>,
    ) -> Result<(), ServiceError> {
        debug!("[{}] <- {}", self.id, name);
        if self.sink.write(msg).is_err() {
            // sink is closed or closing, no chance to send a Close message
            warn!("[{}] Could not send {}, closing connection", self.id, name);
            ctx.stop();
            return Err(ServiceError::NotConnected);
        }
        Ok(())
    }
}

pub fn json_object_from_text_msg(
    id: &str,
    txt: &[u8],
) -> Result<serde_json::Value, serde_json::Error> {
    let msg: serde_json::Value = match serde_json::from_slice(txt) {
        Ok(v) => v,
        Err(e) => {
            warn!("[{}] Error parsing json message: {:?}", id, e);
            return Err(e);
        }
    };

    if !msg.is_object() {
        warn!("[{}] Expected json object but got: {:?}", id, msg);
        return Err(serde_json::Error::custom("expected json object in root"));
    }

    Ok(msg)
}
