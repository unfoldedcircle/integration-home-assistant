// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for Remote Two connection messages.

use crate::controller::{Controller, NewR2Session, R2Session, R2SessionDisconnect};
use actix::{Context, Handler};

impl Handler<NewR2Session> for Controller {
    type Result = ();

    fn handle(&mut self, msg: NewR2Session, _: &mut Context<Self>) -> Self::Result {
        self.sessions
            .insert(msg.id.clone(), R2Session::new(msg.addr));

        self.send_device_state(&msg.id);
    }
}

impl Handler<R2SessionDisconnect> for Controller {
    type Result = ();

    fn handle(&mut self, msg: R2SessionDisconnect, _: &mut Context<Self>) {
        self.sessions.remove(&msg.id);
    }
}
