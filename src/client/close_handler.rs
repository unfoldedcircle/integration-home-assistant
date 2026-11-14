// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for [Close].

use actix::Handler;

use crate::client::HomeAssistantClient;
use crate::client::messages::Close;

impl Handler<Close> for HomeAssistantClient {
    type Result = ();

    fn handle(&mut self, msg: Close, ctx: &mut Self::Context) -> Self::Result {
        self.close(msg, ctx);
    }
}
