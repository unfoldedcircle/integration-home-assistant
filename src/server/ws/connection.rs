// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Actix WebSocket actor for an established Remote Two client connection.

use crate::controller::{NewR2Session, R2SessionDisconnect, SendWsMessage};
use crate::errors::ServiceError;
use crate::server::ws::WsConn;
use actix::{Actor, Addr, Handler};
use actix_ws::{AggregatedMessage, CloseCode, CloseReason, Session};
use log::{debug, error, info, warn};
use std::time::Instant;
use uc_api::ws::{WsMessage, WsResultMsgData};

pub(crate) struct WsSender {
    pub id: String,
    pub session: Session,
    pub msg_tracing_out: bool,
}

impl Actor for WsSender {
    type Context = actix::Context<Self>;
}

impl Handler<SendWsMessage> for WsSender {
    type Result = ();

    fn handle(&mut self, msg: SendWsMessage, _ctx: &mut Self::Context) {
        match serde_json::to_string(&msg.0) {
            Ok(text) => {
                if self.msg_tracing_out {
                    debug!("[{}] <- {text}", self.id);
                }
                let mut session = self.session.clone();
                actix_web::rt::spawn(async move {
                    if let Err(e) = session.text(text).await {
                        error!("WebSocket send error: {e}");
                    }
                });
            }
            Err(e) => error!("[{}] Error serializing outgoing message: {e}", self.id),
        }
    }
}

impl WsConn {
    /// Runs the message processing of the UC Remote WebSocket connection.
    ///
    /// This asynchronous function is responsible for managing the WebSocket
    /// session between the client and the server. It handles the initialization
    /// of the session, processes incoming messages, sends outgoing messages.
    /// This function is meant to run in a Tokio task.
    ///
    /// - Periodically sends WebSocket ping frames to maintain the connection.
    /// - Closes the connection if the heartbeat timeout is exceeded, indicating
    ///   the client is unresponsive.
    /// - If errors occur during registration, outgoing message handling, heartbeat
    ///   checks, or message processing, the WebSocket connection will be closed
    ///   with an appropriate close reason if provided.
    ///
    /// **Actix messages**
    /// - Registers the session with the controller by sending a `NewR2Session`
    ///   message. If this fails, the connection is closed with an error code.
    /// - Incoming WebSocket messages will trigger `R2RequestMsg`, `R2Event`, or `R2ResponseMsg`
    ///   messages to be sent to the controller.
    /// - After the processing loop terminates, the function notifies the
    ///   controller that the session has been closed by sending an
    ///   `R2SessionDisconnect` message.
    ///
    /// # Parameters
    /// - `session`: The `Session` object representing the WebSocket connection
    ///   with the client, used to send and receive messages.
    /// - `stream`: An `AggregatedMessageStream` from `actix_ws` for receiving
    ///   incoming WebSocket messages from the client.
    ///
    pub(crate) async fn run(
        mut self,
        mut session: Session,
        mut stream: actix_ws::AggregatedMessageStream,
    ) {
        use actix_web::rt::time;
        use futures::StreamExt;

        // send initial authentication response
        let auth = serde_json::json!({
            "kind": "resp",
            "req_id": 0,
            "code": 200,
            "msg": "authentication"
        });
        let _ = session.text(auth.to_string()).await;

        // Create sender actor for outgoing messages
        let sender = WsSender {
            id: self.id.clone(),
            session: session.clone(),
            msg_tracing_out: self.msg_tracing_out,
        }
        .start();

        // register new session
        if let Err(e) = self
            .controller_addr
            .send(NewR2Session {
                addr: sender.clone().recipient(),
                id: self.id.clone(),
            })
            .await
        {
            error!("Error registering new WebSocket connection: {e}");
            let _ = session
                .close(Some(CloseReason {
                    code: CloseCode::Error,
                    description: Some("internal error".into()),
                }))
                .await;
            // Can't return an error since this runs in a separate task
            return;
        }

        debug!("[{}] started", self.id);

        let mut hb_interval = time::interval(self.heartbeat.interval);
        loop {
            tokio::select! {
                _ = hb_interval.tick() => {
                    if Instant::now().duration_since(self.hb) > self.heartbeat.timeout {
                        info!("[{}] Closing connection due to failed heartbeat", self.id);
                        self.controller_addr.do_send(R2SessionDisconnect { id: self.id.clone() });
                        let _ = session.close(Some(CloseReason{
                            code: CloseCode::Away,
                            description: Some("heartbeat timeout".into())
                        })).await;
                        break;
                    }
                    let _ = session.ping(b"").await;
                }
                msg = stream.next() => {
                    match msg {
                        Some(Ok(msg)) => {
                            if let Err(close_reason) =
                                self.handle_stream_message(msg, &sender, &mut session).await {
                                    if close_reason.is_some() {
                                        let _ = session.close(close_reason).await;
                                    }
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            info!("[{}] Closing WebSocket: {e:?}", self.id);
                            let _ = session.close(None).await;
                            break;
                        }
                        None => {
                            debug!("[{}] Message stream ended, closing connection", self.id);
                            // Not sure if required, but make sure the WS connection is closed
                            let _ = session.close(None).await;
                            break
                        },
                    }
                }
            }
        }

        // processing loop finished, notify controller that WS connection closed
        self.controller_addr.do_send(R2SessionDisconnect {
            id: self.id.clone(),
        });
        debug!("[{}] stopped", self.id);
    }

    async fn handle_stream_message(
        &mut self,
        msg: AggregatedMessage,
        sender: &Addr<WsSender>,
        session: &mut Session,
    ) -> Result<(), Option<CloseReason>> {
        match msg {
            AggregatedMessage::Text(text) => {
                self.hb = Instant::now();
                if self.msg_tracing_in {
                    debug!("[{}] -> {}", self.id, &text);
                }

                self.handle_text_message(&text, sender).await
            }
            AggregatedMessage::Binary(_) => Err(Some(CloseReason {
                code: CloseCode::Size,
                description: Some("Binary messages not supported!".into()),
            })),
            AggregatedMessage::Ping(bytes) => {
                self.hb = Instant::now();
                let _ = session.pong(&bytes).await;
                Ok(())
            }
            AggregatedMessage::Pong(_) => {
                self.hb = Instant::now();
                Ok(())
            }
            AggregatedMessage::Close(reason) => {
                info!("[{}] Remote closed connection. Reason: {reason:?}", self.id);
                Err(None)
            }
        }
    }

    async fn handle_text_message(
        &self,
        text: &str,
        sender: &Addr<WsSender>,
    ) -> Result<(), Option<CloseReason>> {
        match serde_json::from_str::<WsMessage>(text) {
            Ok(msg) => {
                self.process_ws_message(msg, sender).await;
                Ok(())
            }
            Err(e) => {
                warn!("[{}] Invalid JSON message: {e}", self.id);
                Err(Some(CloseReason {
                    code: CloseCode::Unsupported,
                    description: Some("Invalid JSON message".into()),
                }))
            }
        }
    }

    async fn process_ws_message(&self, msg: WsMessage, sender: &Addr<WsSender>) {
        let req_id = msg.id.unwrap_or_default();
        let req_msg = msg.msg.clone().unwrap_or_default();

        match msg.kind.as_deref() {
            Some("req") => {
                match WsConn::on_request(&self.id, msg, self.controller_addr.clone()).await {
                    Ok(Some(response)) => {
                        sender.do_send(SendWsMessage(response));
                    }
                    Err(e) => {
                        warn!(
                            "[{}] Error processing received message '{req_msg}': {e}",
                            self.id
                        );
                        self.send_error_response(req_id, e, sender);
                    }
                    _ => {}
                }
            }
            Some("resp") => {
                if let Err(e) =
                    WsConn::on_response(&self.id, msg, self.controller_addr.clone()).await
                {
                    warn!("[{}] Error processing response: {e}", self.id);
                }
            }
            Some("event") => {
                if let Err(e) = WsConn::on_event(&self.id, msg, self.controller_addr.clone()).await
                {
                    warn!("[{}] Error processing event: {e}", self.id);
                }
            }
            Some(other) => {
                self.send_error_response(
                    req_id,
                    ServiceError::BadRequest(format!("Unsupported message kind: {other}")),
                    sender,
                );
                warn!("[{}] Unsupported message kind: {other}", self.id);
            }
            None => {
                self.send_error_response(
                    req_id,
                    ServiceError::BadRequest("Missing property: kind".into()),
                    sender,
                );
            }
        }
    }

    fn send_error_response(&self, req_id: u32, error: ServiceError, sender: &Addr<WsSender>) {
        let response = service_error_to_ws_message(&self.id, req_id, error);
        sender.do_send(SendWsMessage(response));
    }
}

fn service_error_to_ws_message(id: &str, req_id: u32, error: ServiceError) -> WsMessage {
    debug!("[{id}] Sending R2 error response for: {error:?}");

    let (code, ws_err) = match error {
        ServiceError::InternalServerError(_) => {
            (500, WsResultMsgData::new("ERROR", "Internal server error"))
        }
        ServiceError::SerializationError(e) => (400, WsResultMsgData::new("BAD_REQUEST", e)),
        ServiceError::BadRequest(e) => (400, WsResultMsgData::new("BAD_REQUEST", e)),
        ServiceError::NotConnected => (
            503,
            WsResultMsgData::new("SERVICE_UNAVAILABLE", "HomeAssistant is not connected"),
        ),
        ServiceError::NotYetImplemented => (
            501,
            WsResultMsgData::new("NOT_IMPLEMENTED", "Not yet implemented"),
        ),
        ServiceError::ServiceUnavailable(e) => {
            (503, WsResultMsgData::new("SERVICE_UNAVAILABLE", e))
        }
        ServiceError::NotFound(e) => (404, WsResultMsgData::new("NOT_FOUND", e)),
    };

    WsMessage::error(req_id, code, ws_err)
}
