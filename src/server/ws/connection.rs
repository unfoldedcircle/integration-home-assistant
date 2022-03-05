// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use crate::messages::{NewR2Session, R2SessionDisconnect};
use crate::server::ws::api_messages::WsMessage;
use crate::server::ws::{api_messages, WsConn};
use crate::Controller;

use actix::{
    fut, Actor, ActorContext, ActorFutureExt, Addr, AsyncContext, ContextFutureSpawner, Handler,
    Running, StreamHandler, WrapFuture,
};
use actix_web_actors::ws::{CloseCode, CloseReason, Message, ProtocolError, WebsocketContext};
use bytestring::ByteString;
use log::{debug, error, info, warn};
use std::time::{Duration, Instant};

// TODO make configurable?
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

impl Actor for WsConn {
    type Context = WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.start_heartbeat(ctx);
        // register new WebSocket connection to our handler
        self.controller_addr
            .send(NewR2Session {
                addr: ctx.address().recipient(),
                id: self.id.clone(),
            })
            .into_actor(self)
            .then(|res, _, ctx| {
                match res {
                    Ok(_res) => (),
                    _ => ctx.stop(),
                }
                fut::ready(())
            })
            .wait(ctx);

        debug!("started");
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        // remove WebSocket connection from our handler
        self.controller_addr.do_send(R2SessionDisconnect {
            id: self.id.clone(),
        });
        info!("stopped");
        Running::Stop
    }
}

impl StreamHandler<actix_web::Result<Message, ProtocolError>> for WsConn {
    fn handle(&mut self, msg: actix_web::Result<Message, ProtocolError>, ctx: &mut Self::Context) {
        if let Ok(msg) = msg {
            match msg {
                Message::Text(text) => self.on_text_message(text, ctx),
                Message::Binary(_) => {
                    self.close(CloseCode::Size, "Binary messages not supported!", ctx);
                }
                Message::Ping(bytes) => {
                    self.hb = Instant::now();
                    ctx.pong(&bytes);
                }
                Message::Pong(_) => self.hb = Instant::now(),
                Message::Close(reason) => {
                    ctx.close(reason);
                    ctx.stop();
                }
                Message::Continuation(_) => {
                    self.close(CloseCode::Size, "Continuation frames not supported!", ctx);
                }
                Message::Nop => {}
            }
        } else {
            info!("Closing WebSocket: {:?}", msg.unwrap_err());
            ctx.stop();
        }
    }
}

impl Handler<WsMessage> for WsConn {
    type Result = ();

    fn handle(&mut self, msg: WsMessage, ctx: &mut Self::Context) {
        if let Ok(msg) = serde_json::to_string(&msg) {
            ctx.text(msg);
        } else {
            error!("Error serializing {:?}", msg)
        }
    }
}

impl WsConn {
    pub(crate) fn new(client_id: String, controller_addr: Addr<Controller>) -> Self {
        Self {
            id: client_id,
            hb: Instant::now(),
            controller_addr,
        }
    }

    fn start_heartbeat(&self, ctx: &mut WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // TODO check if we got standby event from remote: suspend until out of standby and then test connection
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                info!("[{}] Closing connection due to failed heartbeat", act.id);
                // remove WebSocket connection from our handler
                act.controller_addr
                    .do_send(R2SessionDisconnect { id: act.id.clone() });

                ctx.stop();
                return;
            }

            ctx.ping(b"");
        });
    }

    fn close(&mut self, code: CloseCode, description: &str, ctx: &mut WebsocketContext<WsConn>) {
        info!("Closing connection with code {:?}: {}", code, description);
        ctx.close(Some(CloseReason {
            code,
            description: Some(description.into()),
        }));
        ctx.stop();
    }

    pub(crate) fn send_error(
        &self,
        req_id: u32,
        code: u16,
        error_code: &str,
        message: String,
        ctx: &mut WebsocketContext<WsConn>,
    ) {
        let data = api_messages::WsError {
            code: error_code.into(),
            message,
        };
        let response = api_messages::WsResponse::error(req_id, code, data);
        if let Ok(msg) = serde_json::to_string(&response) {
            ctx.text(msg);
        }
    }

    pub(crate) fn send_missing_field_error(
        &self,
        req_id: u32,
        field: &str,
        ctx: &mut WebsocketContext<WsConn>,
    ) {
        let response = api_messages::WsResponse::missing_field(req_id, field);
        if let Ok(msg) = serde_json::to_string(&response) {
            ctx.text(msg);
        }
    }

    fn on_text_message(&mut self, text: ByteString, ctx: &mut WebsocketContext<WsConn>) {
        let msg: api_messages::WsMessage = match serde_json::from_slice(text.as_ref()) {
            Ok(v) => v,
            Err(e) => {
                warn!("[{}] Invalid JSON message: {}", self.id, e.to_string());
                self.close(CloseCode::Unsupported, "Invalid JSON message", ctx);
                return;
            }
        };

        match msg.kind {
            None => {
                warn!(
                    "[{}] Expected json object payload with 'kind' key, but got: {:?}",
                    self.id, text
                );
                self.send_missing_field_error(0, "kind", ctx);
            }
            Some(ref k) => match k.as_str() {
                "req" => self.on_request(msg, ctx),
                "resp" => self.on_response(msg, ctx),
                "event" => self.on_event(msg, ctx),
                _ => {
                    warn!("[{}] Unsupported client message kind: {}", self.id, k);
                    self.send_error(
                        0,
                        400,
                        "BAD_REQUEST",
                        format!("Invalid kind value: {}", k),
                        ctx,
                    );
                }
            },
        }
    }
}
