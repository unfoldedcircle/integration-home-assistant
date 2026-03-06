// Copyright (c) 2026 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home Assistant media player queue implementation.

use crate::client::HomeAssistantClient;
use crate::client::messages::GetMediaQueue;
use crate::errors::ServiceError;
use actix::Handler;

impl Handler<GetMediaQueue> for HomeAssistantClient {
    type Result = Result<(), ServiceError>;

    // TODO(media-browsing) implement media queue with Music Assistant
    fn handle(&mut self, _msg: GetMediaQueue, _ctx: &mut Self::Context) -> Self::Result {
        // HA doesn't have a standard WebSocket command to get the queue for a media player.
        // The best option is probably to support Media Assistant

        Err(ServiceError::NotYetImplemented)
    }
}
