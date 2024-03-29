// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix message handler for [R2ResponseMsg].

use crate::controller::{Controller, R2ResponseMsg};
use actix::Handler;
use log::info;

impl Handler<R2ResponseMsg> for Controller {
    type Result = ();

    fn handle(&mut self, msg: R2ResponseMsg, _ctx: &mut Self::Context) -> Self::Result {
        info!(
            "[{}] TODO implement remote response: {}",
            msg.ws_id, msg.msg
        );
    }
}
