// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix actor handler implementation for the `Close` message

use crate::client::messages::Close;
use crate::client::HomeAssistantClient;

use actix::{ActorContext, AsyncContext, Handler};
use actix_web_actors::ws;
use awc::ws::CloseReason;
use log::info;
use std::time::Duration;

impl Handler<Close> for HomeAssistantClient {
    type Result = ();

    fn handle(&mut self, msg: Close, ctx: &mut Self::Context) -> Self::Result {
        info!("[{}] Close msg: sending Close to HomeAssistant", self.id);
        // Try graceful shutdown first: we'll receive a Close frame back from the server which will Stop the context.
        // If send_message fails the actor will be closed.
        if self
            .send_message(
                ws::Message::Close(Some(CloseReason {
                    code: msg.code,
                    description: msg.description,
                })),
                "Close",
                ctx,
            )
            .is_ok()
        {
            // Then a hard disconnect as safety net if the connection is stale
            ctx.run_later(Duration::from_millis(100), move |act, ctx| {
                info!("[{}] Force stopping actor", act.id);
                act.sink.close();
                ctx.stop();
            });
        }
    }
}
