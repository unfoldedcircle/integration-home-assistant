// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! WebSocket server for the Remote Two integration API

use crate::Controller;
use crate::configuration::{ENV_API_MSG_TRACING, HeartbeatSettings, WebSocketSettings};
use actix::Addr;
use actix_web::error::JsonPayloadError;
use actix_web::{Error, HttpRequest, HttpResponse, error, get, web};
use log::{debug, info};
use std::env;
use std::time::Instant;
use uc_api::core::web::ApiResponse;
use uuid::Uuid;

mod connection;
mod events;
mod requests;
mod responses;

/// WebSocket connection instance and Actix WebSocket actor.
struct WsConn {
    /// Unique connection identifier.
    ///
    /// Used to associate received messages when passing them to the [`Controller`] and for logging
    /// purposes.
    id: String,
    /// Heartbeat timestamp of last activity.
    hb: Instant,
    /// [`Controller`] actix address for sending WS events & requests.
    controller_addr: Addr<Controller>,
    heartbeat: HeartbeatSettings,
    /// Enable incoming websocket message tracing: log every message. SECRETS ARE EXPOSED!
    msg_tracing_in: bool,
    /// Enable outgoing websocket message tracing: log every message
    msg_tracing_out: bool,
}

impl WsConn {
    fn new(
        client_id: String,
        controller_addr: Addr<Controller>,
        heartbeat: HeartbeatSettings,
    ) -> Self {
        let msg_tracing = env::var(ENV_API_MSG_TRACING).unwrap_or_default();
        Self {
            id: client_id,
            hb: Instant::now(),
            controller_addr,
            heartbeat,
            msg_tracing_in: msg_tracing == "all" || msg_tracing == "in",
            msg_tracing_out: msg_tracing == "all" || msg_tracing == "out",
        }
    }
}

/// HTTP endpoint for the WebSocket upgrade
#[get("/ws")]
pub async fn ws_index(
    request: HttpRequest,
    stream: web::Payload,
    websocket_settings: web::Data<WebSocketSettings>,
    controller: web::Data<Addr<Controller>>,
) -> actix_web::Result<HttpResponse, Error> {
    let client_addr = request.peer_addr().map(|p| p.to_string());
    // Note: don't print full request, it may contain an auth-token header!
    let client = client_addr.as_deref().unwrap_or("?");
    debug!("New WebSocket connection from: {client}");

    // Authenticate connection if a token is configured
    if websocket_settings.token.is_some() {
        let auth_token = request
            .headers()
            .get("auth-token")
            .and_then(|v| match v.to_str() {
                Ok(v) => Some(v.to_string()),
                Err(_) => None,
            });

        if auth_token != websocket_settings.token {
            info!("Invalid token, closing client connection {client}");
            return Ok(HttpResponse::Unauthorized()
                .json(ApiResponse::new("ERROR", "Authentication failed")));
        }
    }

    // TODO limit number of active ws sessions?
    // use peer IP:port as unique client identifier
    let client_id = request
        .peer_addr()
        .map(|addr| format!("{}:{}", addr.ip(), addr.port()))
        .unwrap_or_else(|| Uuid::new_v4().as_hyphenated().to_string());

    let (resp, session, msg_stream) = actix_ws::handle(&request, stream)?;

    // Increase the maximum allowed frame size to 128KiB. Default is 64KiB.
    // Note: there shouldn't be a need to increase this, since the Remote request message payloads are small.
    // Also, aggregate continuation frames is just "future proofing" and not used at the moment in the Remote.
    let stream = msg_stream
        .max_frame_size(128 * 1024)
        .aggregate_continuations();

    let conn = WsConn::new(
        client_id,
        controller.get_ref().clone(),
        websocket_settings.heartbeat,
    );

    actix_web::rt::spawn(async move {
        conn.run(session, stream).await;
    });

    Ok(resp)
}

/// Custom Actix Web error handler
pub fn json_error_handler(err: JsonPayloadError, _: &HttpRequest) -> Error {
    let message = err.to_string();

    let resp = match &err {
        JsonPayloadError::ContentType => HttpResponse::UnsupportedMediaType()
            .json(ApiResponse::new("UNSUPPORTED_MEDIA_TYPE", &message[..])),
        JsonPayloadError::Deserialize(json_err) if json_err.is_data() => {
            // alternative: HttpResponse::UnprocessableEntity 422
            HttpResponse::BadRequest().json(ApiResponse::new("INVALID_JSON", &message[..]))
        }
        _ => HttpResponse::BadRequest().json(ApiResponse::new("BAD_REQUEST", &message[..])),
    };

    error::InternalError::from_response(err, resp).into()
}
