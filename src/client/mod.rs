// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home Assistant client WebSocket API implementation with Actix actors.

use std::env;
use std::time::Instant;

use actix::io::SinkWrite;
use actix::{Actor, ActorContext, Addr, AsyncContext, Context};
use actix_codec::Framed;
use awc::{ws, BoxedSocket};
use bytes::Bytes;
use futures::stream::{SplitSink, SplitStream};
use log::{debug, error, info, warn};
use messages::Close;
use serde::de::Error;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};
use url::Url;

use crate::client::messages::{ConnectionEvent, ConnectionState};
use crate::client::model::Event;
use crate::configuration::{HeartbeatSettings, ENV_HASS_MSG_TRACING};
use crate::errors::ServiceError;
use crate::Controller;

mod actor;
mod close_handler;
mod entity;
mod event;
mod get_states;
pub mod messages;
mod model;
mod service;
mod streamhandler;

static CLIENT_SEQ: AtomicU32 = AtomicU32::new(1);

pub struct HomeAssistantClient {
    /// Unique HA client id
    id: String,
    /// Base server address for media image access (e.g. <http://hassio.local:8123>)
    server: Url,
    /// HA request message id
    ws_id: u32,
    access_token: String,
    uc_ha_component: bool,
    /// True if custom HA component is detected and will use optimized workflows
    uc_ha_component_info_id: Option<u32>,
    /// request id of the last `subscribe_events` request. This id will be used in the result and event messages.
    subscribed_events: bool,
    /// request id of the last `subscribe_events` request. This id will be used in the result and event messages.
    subscribe_events_id: Option<u32>,
    /// request id of the last `subscribe_events` request. This id will be used the result message.
    entity_states_id: Option<u32>,
    sink: SinkWrite<ws::Message, SplitSink<Framed<BoxedSocket, ws::Codec>, ws::Message>>,
    controller_actor: Addr<Controller>,
    /// Last heart beat timestamp.
    last_hb: Instant,
    heartbeat: HeartbeatSettings,
    /// Enable incoming websocket message tracing: log every message.
    msg_tracing_in: bool,
    /// Enable outgoing websocket message tracing: log every message, except messages with key
    /// `access_token`.
    msg_tracing_out: bool,
}

impl HomeAssistantClient {
    pub fn start(
        url: Url,
        controller_actor: Addr<Controller>,
        access_token: String,
        sink: SplitSink<Framed<BoxedSocket, ws::Codec>, ws::Message>,
        stream: SplitStream<Framed<BoxedSocket, ws::Codec>>,
        heartbeat: HeartbeatSettings,
    ) -> Addr<Self> {
        HomeAssistantClient::create(|ctx| {
            ctx.add_stream(stream);
            let scheme = url.scheme();
            let host = url.host_str().unwrap_or(url.as_str());
            let port = url.port_or_known_default().unwrap_or_default();
            let msg_tracing = env::var(ENV_HASS_MSG_TRACING).unwrap_or_default();
            HomeAssistantClient {
                id: format!(
                    "{}:{}-{}",
                    host,
                    port,
                    CLIENT_SEQ.fetch_add(1, Ordering::SeqCst)
                ),
                server: {
                    let mut server = url.clone();
                    server
                        .set_scheme(if scheme == "wss" { "https" } else { "http" })
                        .expect("invalid scheme");
                    server.set_path("");
                    server
                },
                ws_id: 0,
                access_token,
                subscribed_events: false,
                subscribe_events_id: None,
                entity_states_id: None,
                sink: SinkWrite::new(sink, ctx),
                controller_actor,
                last_hb: Instant::now(),
                heartbeat,
                msg_tracing_in: msg_tracing == "all" || msg_tracing == "in",
                msg_tracing_out: msg_tracing == "all" || msg_tracing == "out",
                uc_ha_component: false,
                uc_ha_component_info_id: None,
            }
        })
    }

    fn new_msg_id(&mut self) -> u32 {
        self.ws_id += 1;
        self.ws_id
    }

    fn heartbeat(&self, ctx: &mut Context<Self>) {
        if self.heartbeat.interval.is_zero() {
            warn!("[{}] Websocket server heartbeat is disabled", self.id);
            return;
        }

        ctx.run_later(self.heartbeat.interval, |act, ctx| {
            // check server heartbeats
            if !act.heartbeat.timeout.is_zero()
                && Instant::now().duration_since(act.last_hb) > act.heartbeat.timeout
            {
                // heartbeat timed out
                error!(
                    "[{}] Websocket server heartbeat failed, disconnecting!",
                    act.id
                );

                // Stop sending pings & Stop actor
                ctx.stop();
                return;
            }

            let msg = if act.heartbeat.ping_frames {
                ws::Message::Ping(Bytes::new())
            } else {
                let id = act.new_msg_id();
                ws::Message::Text(json!({"id": id, "type": "ping"}).to_string().into())
            };
            if act.send_message(msg, "Ping", ctx).is_ok() {
                act.heartbeat(ctx);
            }
        });
    }

    fn on_text_message(&mut self, txt: Bytes, ctx: &mut Context<HomeAssistantClient>) {
        if self.msg_tracing_in {
            debug!("[{}] -> {}", self.id, String::from_utf8_lossy(txt.as_ref()));
        }

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

                if Some(id) == self.uc_ha_component_info_id {
                    // If the uc/info message type is unknown, the UC HA component is not
                    // installed then we switch back to standard HA events
                    if !success {
                        if !self.subscribed_events {
                            self.subscribe_events_id = Some(self.new_msg_id());
                            if let Err(e) = self.send_json(
                                json!({
                          "id": self.subscribe_events_id.unwrap(),
                          "type": "subscribe_events",
                          "event_type": "state_changed"
                        }),
                                ctx,
                            ) {
                                error!(
                            "[{}] Error sending subscribe_events to HA: {:?}",
                            self.id, e
                        );
                                ctx.notify(Close::invalid());
                            }
                        }
                        return;
                    }
                    self.uc_ha_component = true;
                    //TODO subscribe events for each subscribed entities
                    //TODO : 1 subscribe event per entity of group subscriptions ?
                    //Better to group subscriptions and each modification of subscribed entities
                    //will erase and recreate subscription
                } else if Some(id) == self.subscribe_events_id {
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

                    if let Some(entities) =
                        object_msg.get_mut("result").and_then(|v| v.as_array_mut())
                    {
                        // this looks ugly! Is there a better way to get ownership of the array?
                        let entities: Vec<Value> = entities.iter_mut().map(|v| v.take()).collect();
                        if let Err(e) = self.handle_get_states_result(entities) {
                            error!("[{}] Error handling HA get_states result: {:?}", self.id, e);
                        }
                    }
                }
            }
            "auth_required" => {
                if let Err(e) = self.send_json(
                    json!({ "type": "auth", "access_token": self.access_token}),
                    ctx,
                ) {
                    error!("[{}] Error sending auth to HA: {:?}", self.id, e);
                    ctx.notify(Close::invalid());
                }
            }
            "auth_invalid" => {
                error!(
                    "[{}] Invalid authentication! {}",
                    self.id,
                    object_msg
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                );
                self.controller_actor.do_send(ConnectionEvent {
                    client_id: self.id.clone(),
                    state: ConnectionState::AuthenticationFailed,
                });
            }
            "auth_ok" => {
                info!(
                    "[{}] Authentication OK. HA version: {}",
                    self.id,
                    object_msg
                        .get("ha_version")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                );

                // Instead of subscribing to standard events which sends events from all entities
                // we check after the UC HA component then fall back to standard HA events
                if !self.uc_ha_component {
                    self.uc_ha_component_info_id = Some(self.new_msg_id());
                    if let Err(e) = self.send_json(
                        json!({
                          "id": self.uc_ha_component_info_id,
                          "type": "uc/info"
                        }),
                        ctx,
                    ) {
                        debug!(
                            "[{}] UC Home assistant component not installed. Switching to standard HA: {:?}",
                            self.id, e
                        );
                    }
                }
            }
            "pong" => self.last_hb = Instant::now(),
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

    fn send_json(
        &mut self,
        msg: Value,
        ctx: &mut Context<HomeAssistantClient>,
    ) -> Result<(), ServiceError> {
        let obj = msg.as_object().ok_or(ServiceError::BadRequest(
            "json message must be an object".into(),
        ))?;
        let name = obj.get("type").and_then(|v| v.as_str()).unwrap_or("?");
        let msg = msg.to_string();
        // hide access token in tracing mode
        if self.msg_tracing_out && !obj.contains_key("access_token") {
            debug!("[{}] <- {msg}", self.id);
        } else {
            debug!("[{}] <- {name}", self.id);
        }
        if self.sink.write(ws::Message::Text(msg.into())).is_err() {
            // sink is closed or closing, no chance to send a Close message
            warn!("[{}] Could not send {name}, closing connection", self.id);
            ctx.stop();
            return Err(ServiceError::NotConnected);
        }
        Ok(())
    }

    fn send_message(
        &mut self,
        msg: ws::Message,
        name: &str,
        ctx: &mut Context<HomeAssistantClient>,
    ) -> Result<(), ServiceError> {
        if self.msg_tracing_out {
            if let ws::Message::Text(txt) = &msg {
                debug!("[{}] <- {txt}", self.id);
            } else {
                debug!("[{}] <- {:?}", self.id, msg);
            }
        } else {
            debug!("[{}] <- {}", self.id, name);
        }
        if self.sink.write(msg).is_err() {
            // sink is closed or closing, no chance to send a Close message
            warn!("[{}] Could not send {}, closing connection", self.id, name);
            ctx.stop();
            return Err(ServiceError::NotConnected);
        }
        Ok(())
    }
}

pub fn json_object_from_text_msg(id: &str, txt: &[u8]) -> Result<Value, serde_json::Error> {
    let msg: Value = match serde_json::from_slice(txt) {
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
