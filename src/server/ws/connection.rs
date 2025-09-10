// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix WebSocket actor for an established Remote Two client connection.

use crate::controller::{NewR2Session, R2SessionDisconnect, SendWsMessage};
use crate::errors::ServiceError;
use crate::server::ws::WsConn;
use actix::{
    Actor, ActorContext, ActorFutureExt, AsyncContext, ContextFutureSpawner, Handler,
    ResponseActFuture, Running, StreamHandler, WrapFuture, fut,
};
use actix_web_actors::ws::{CloseCode, CloseReason, Message, ProtocolError, WebsocketContext};
use bytestring::ByteString;
use log::{debug, error, info, warn};
use std::time::Instant;
use uc_api::ws::{WsMessage, WsResultMsgData};

/// Local Actix message to handle WebSocket text message.
///
/// This is a "one way" fire and forget message on purpose to simplify handling in the StreamHandler.
/// Any errors must be handled in the receiver, e.g. sending error response messages back to
/// the Remote Two!
#[derive(actix::prelude::Message)]
#[rtype(result = "()")]
struct TextMsg(pub ByteString);

impl Actor for WsConn {
    type Context = WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // Noticed `send failed because receiver is full` errors with the default 16.
        // Likely due to rapid entity updates from HA.
        ctx.set_mailbox_capacity(32);

        self.start_heartbeat(ctx);

        // since we only implemented the header based authentication in server::ws_index we can send
        // the authentication event right after startup
        let json = serde_json::json!({
            "kind": "resp",
            "req_id": 0,
            "code": 200,
            "msg": "authentication"
        });
        ctx.text(json.to_string());

        // register new WebSocket connection to our handler
        self.controller_addr
            .send(NewR2Session {
                addr: ctx.address().recipient(),
                id: self.id.clone(),
            })
            .into_actor(self)
            .then(|res, _, ctx| {
                if let Err(e) = res {
                    error!("Error registering new WebSocket connection: {e}");
                    ctx.stop();
                }
                fut::ready(())
            })
            .wait(ctx);

        debug!("[{}] started", self.id);
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        // remove WebSocket connection from our handler
        self.controller_addr.do_send(R2SessionDisconnect {
            id: self.id.clone(),
        });
        debug!("[{}] stopped", self.id);
        Running::Stop
    }
}

impl StreamHandler<actix_web::Result<Message, ProtocolError>> for WsConn {
    fn handle(&mut self, msg: actix_web::Result<Message, ProtocolError>, ctx: &mut Self::Context) {
        if let Ok(msg) = msg {
            match msg {
                Message::Text(text) => ctx.notify(TextMsg(text)),
                Message::Binary(_) => {
                    self.close(CloseCode::Size, "Binary messages not supported!", ctx);
                }
                Message::Ping(bytes) => {
                    self.hb = Instant::now();
                    ctx.pong(&bytes);
                }
                Message::Pong(_) => self.hb = Instant::now(),
                Message::Close(reason) => {
                    info!("[{}] Remote closed connection. Reason: {reason:?}", self.id);
                    ctx.stop();
                }
                Message::Continuation(_) => {
                    self.close(CloseCode::Size, "Continuation frames not supported!", ctx);
                }
                Message::Nop => {}
            }
        } else {
            info!("[{}] Closing WebSocket: {:?}", self.id, msg.unwrap_err());
            ctx.stop();
        }
    }
}

impl Handler<TextMsg> for WsConn {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, text: TextMsg, ctx: &mut Self::Context) -> Self::Result {
        if self.msg_tracing_in {
            debug!("[{}] -> {}", self.id, text.0);
        }

        let msg: WsMessage = match serde_json::from_slice(text.0.as_ref()) {
            Ok(v) => v,
            Err(e) => {
                warn!("[{}] Invalid JSON message: {e}", self.id);
                self.close(CloseCode::Unsupported, "Invalid JSON message", ctx);
                return Box::pin(fut::ready(()));
            }
        };

        // clone required data for async context
        let req_id = msg.id.unwrap_or_default();
        let req_msg = msg.msg.clone().unwrap_or_default();
        let session_id = self.id.clone();
        let controller_addr = self.controller_addr.clone();

        Box::pin(
            async move {
                match msg.kind {
                    None => Err(ServiceError::BadRequest("Missing property: kind".into())),
                    Some(ref k) => match k.as_str() {
                        "req" => WsConn::on_request(&session_id, msg, controller_addr).await,
                        "resp" => {
                            WsConn::on_response(&session_id, msg, controller_addr).await?;
                            Ok(None)
                        }
                        "event" => {
                            WsConn::on_event(&session_id, msg, controller_addr).await?;
                            Ok(None)
                        }
                        _ => Err(ServiceError::BadRequest(format!(
                            "Unsupported message kind: {k}"
                        ))),
                    },
                }
            }
            .into_actor(self) // converts future to ActorFuture
            .map(
                move |result: Result<Option<WsMessage>, ServiceError>, act, ctx| match result {
                    Ok(Some(response)) => {
                        ctx.notify(SendWsMessage(response));
                    }
                    Err(e) => {
                        warn!(
                            "[{}] Error processing received message '{req_msg}': {e}",
                            act.id
                        );
                        let response = service_error_to_ws_message(&act.id, req_id, e);
                        ctx.notify(SendWsMessage(response));
                    }
                    _ => {}
                },
            ),
        )
    }
}

impl Handler<SendWsMessage> for WsConn {
    type Result = ();

    fn handle(&mut self, msg: SendWsMessage, ctx: &mut Self::Context) {
        if let Ok(msg) = serde_json::to_string(&msg.0) {
            if self.msg_tracing_out {
                debug!("[{}] <- {msg}", self.id);
            }
            ctx.text(msg);
        } else {
            error!("[{}] Error serializing {:?}", self.id, msg.0)
        }
    }
}

impl WsConn {
    fn start_heartbeat(&self, ctx: &mut WebsocketContext<Self>) {
        ctx.run_interval(self.heartbeat.interval, |act, ctx| {
            if Instant::now().duration_since(act.hb) > act.heartbeat.timeout {
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
        info!(
            "[{}] Closing connection with code {code:?}: {description}",
            self.id
        );
        ctx.close(Some(CloseReason {
            code,
            description: Some(description.into()),
        }));
        ctx.stop();
    }
}

fn service_error_to_ws_message(id: &str, req_id: u32, error: ServiceError) -> WsMessage {
    debug!("[{id}] Sending R2 error response for: {error:?}");

    let (code, ws_err) = match error {
        ServiceError::InternalServerError(_) => {
            (500, WsResultMsgData::new("ERROR", "Internal server error"))
        }
        ServiceError::SerializationError(e) => (400, WsResultMsgData::new("BAD_REQUEST", e)),
        ServiceError::BadRequest(e) => (400, WsResultMsgData::new("BAD_REQUEST", e)),
        ServiceError::NotConnected => (
            503,
            WsResultMsgData::new("SERVICE_UNAVAILABLE", "HomeAssistant is not connected"),
        ),
        ServiceError::NotYetImplemented => (
            501,
            WsResultMsgData::new("NOT_IMPLEMENTED", "Not yet implemented"),
        ),
        ServiceError::ServiceUnavailable(e) => {
            (503, WsResultMsgData::new("SERVICE_UNAVAILABLE", e))
        }
        ServiceError::NotFound(e) => (404, WsResultMsgData::new("NOT_FOUND", e)),
    };

    WsMessage::error(req_id, code, ws_err)
}
