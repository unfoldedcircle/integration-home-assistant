// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use crate::configuration::WebSocketSettings;
use crate::Controller;

use actix::Addr;
use actix_web::{get, web, Error, HttpRequest, HttpResponse, Result};
use log::debug;
use uuid::Uuid;
use web_model::ApiResponse;
pub use ws::api_messages::*;
use ws::WsConn;

pub mod web_model;
mod ws;

#[get("/ws")]
pub async fn ws_index(
    request: HttpRequest,
    stream: web::Payload,
    websocket_settings: web::Data<WebSocketSettings>,
    controller: web::Data<Addr<Controller>>,
) -> Result<HttpResponse, Error> {
    debug!("New WebSocket connection: {:?}", request);

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
            return Ok(HttpResponse::Unauthorized()
                .json(ApiResponse::new("ERROR", "Authentication failed")));
        }
    }

    // TODO limit number of active ws sessions?
    // use peer IP:port as unique client identifier
    let client_id = request
        .peer_addr()
        .map(|addr| format!("{}:{}", addr.ip(), addr.port()))
        .unwrap_or_else(|| Uuid::new_v4().to_hyphenated().to_string());

    actix_web_actors::ws::start(
        WsConn::new(client_id, controller.get_ref().clone()),
        &request,
        stream,
    )
}
