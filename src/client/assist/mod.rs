// Copyright (c) 2025 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use crate::client::HomeAssistantClient;
use crate::controller::R2AudioChunkMsg;
use crate::errors::ServiceError;
use actix::Handler;
use awc::ws;
use bytes::{BufMut, BytesMut};

mod call_pipeline;

pub const DEF_SAMPLE_RATE: u32 = 16000;

impl Handler<R2AudioChunkMsg> for HomeAssistantClient {
    type Result = Result<(), ServiceError>;

    fn handle(&mut self, msg: R2AudioChunkMsg, ctx: &mut Self::Context) -> Self::Result {
        let (_, session) = self
            .assist_sessions
            .iter()
            .find(|(_, session)| session.session_id == msg.session_id)
            .ok_or_else(|| {
                ServiceError::BadRequest(format!(
                    "No HA assist session found for session id {}",
                    msg.session_id
                ))
            })?;

        let bin_id = session
            .stt_binary_handler_id
            .ok_or(ServiceError::BadRequest("No binary handler id".into()))?;

        let mut buffer = BytesMut::with_capacity(msg.data.len() + 1);
        buffer.put_u8(bin_id);
        buffer.put_slice(&msg.data);

        self.send_message(ws::Message::Binary(buffer.into()), "Audio", ctx)?;

        Ok(())
    }
}
