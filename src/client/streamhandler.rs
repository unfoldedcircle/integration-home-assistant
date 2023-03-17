// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! StreamHandler trait implementation to receive WebSocket frames.

use actix::{ActorContext, AsyncContext, Context, StreamHandler};
use actix_web_actors::ws::{CloseCode, Frame, ProtocolError as WsProtocolError};
use log::{debug, error, info};

use crate::client::messages::Close;
use crate::client::HomeAssistantClient;

impl StreamHandler<Result<Frame, WsProtocolError>> for HomeAssistantClient {
    fn handle(&mut self, msg: Result<Frame, WsProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Err(e) => {
                error!("[{}] Protocol error: {e}", self.id);
                ctx.notify(Close {
                    code: CloseCode::Protocol,
                    description: Some(e.to_string()),
                });
                return;
            }
            Ok(msg) => msg,
        };

        match msg {
            Frame::Text(txt) => self.on_text_message(txt, ctx),
            Frame::Binary(bytes) => self.on_binary_message(bytes, ctx),
            Frame::Ping(b) => self.on_ping_message(b, ctx),
            Frame::Pong(b) => self.on_pong_message(b, ctx),
            Frame::Close(c) => {
                info!("[{}] HA closed connection. Reason: {c:?}", self.id);
                self.sink.close();
                ctx.stop();
            }
            Frame::Continuation(_) => {
                error!(
                    "[{}] Continuation frames not supported! Disconnecting",
                    self.id
                );
                ctx.notify(Close::unsupported());
            }
        }
    }

    fn started(&mut self, _: &mut Context<Self>) {
        debug!("[{}] HA StreamHandler connected", self.id);
    }

    fn finished(&mut self, ctx: &mut Context<Self>) {
        debug!("[{}] HA StreamHandler disconnected", self.id);
        ctx.stop()
    }
}

impl actix::io::WriteHandler<WsProtocolError> for HomeAssistantClient {}
