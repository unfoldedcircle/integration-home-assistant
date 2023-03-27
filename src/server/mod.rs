// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Server modules of the integration driver. Handling WebSocket and mDNS advertisement.

mod mdns;
mod ws;

pub use mdns::publish_service;
pub use ws::{json_error_handler, ws_index};
