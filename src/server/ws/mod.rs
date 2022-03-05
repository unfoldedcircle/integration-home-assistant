// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use crate::Controller;

use actix::Addr;
use std::time::Instant;

pub mod api_messages;
mod connection;
mod events;
mod requests;
mod responses;

pub struct WsConn {
    /// Connection identifier
    id: String,
    /// Heartbeat timestamp of last activity
    hb: Instant,
    controller_addr: Addr<Controller>,
}
