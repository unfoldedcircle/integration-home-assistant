// Copyright (c) 2026 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home Assistant connection lifecycle tests against a local fake Home Assistant WebSocket
//! server: authentication, reconnection, giving up, disconnecting and the heartbeat.
//!
//! The tests drive the [`Controller`] like the Remote does and observe the `device_state` events
//! the Remote receives.

use actix::prelude::*;
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::Message as WsFrame;
use uc_api::intg::ws::R2Event;
use uc_intg_hass::configuration::{Settings, get_driver_metadata};
use uc_intg_hass::{Controller, NewR2Session, R2EventMsg, SendWsMessage};
use url::Url;

const TOKEN: &str = "test-token";

/// Scripted behaviour and recorded activity of the fake Home Assistant server.
#[derive(Default)]
struct FakeHa {
    /// Recorded activity, e.g. `connect`, `auth:<token>`, `msg:<type>`, `close:<code>`.
    log: Mutex<Vec<String>>,
    /// Answer the authentication with `auth_invalid`.
    reject_auth: AtomicBool,
    /// Don't answer `ping` API messages.
    ignore_pings: AtomicBool,
    /// Send a message which is not valid JSON after the next successful authentication.
    send_garbage: AtomicBool,
    /// Refuse new connections: close them before the WebSocket handshake.
    refuse_connections: AtomicBool,
    /// Drops the current connection without a close frame.
    drop_connection: Notify,
}

impl FakeHa {
    fn record(&self, entry: impl Into<String>) {
        self.log.lock().unwrap().push(entry.into());
    }

    fn count(&self, entry: &str) -> usize {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|e| *e == entry)
            .count()
    }
}

/// Starts the fake Home Assistant server. Returns its WebSocket URL.
async fn spawn_fake_ha(ha: Arc<FakeHa>) -> Url {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            if ha.refuse_connections.load(Ordering::SeqCst) {
                ha.record("refused");
                continue; // drops the TCP connection
            }
            let ha = ha.clone();
            tokio::spawn(async move {
                let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                    return;
                };
                ha.record("connect");
                let _ = ws
                    .send(text(
                        json!({"type": "auth_required", "ha_version": "2026.1.0"}),
                    ))
                    .await;
                loop {
                    tokio::select! {
                        _ = ha.drop_connection.notified() => {
                            ha.record("dropped");
                            return;
                        }
                        frame = ws.next() => {
                            let Some(Ok(frame)) = frame else { return };
                            match frame {
                                WsFrame::Text(msg) => {
                                    let msg: Value = serde_json::from_str(msg.as_str()).unwrap();
                                    for reply in ha.reply(&msg) {
                                        let auth_invalid = reply["type"] == "auth_invalid";
                                        let _ = ws.send(text(reply.clone())).await;
                                        if reply["type"] == "auth_ok"
                                            && ha.send_garbage.swap(false, Ordering::SeqCst)
                                        {
                                            let _ = ws.send(WsFrame::Text("no json".into())).await;
                                        }
                                        if auth_invalid {
                                            // Home Assistant disconnects the client
                                            ha.record("auth_invalid");
                                            let _ = ws.close(None).await;
                                            return;
                                        }
                                    }
                                }
                                WsFrame::Close(close) => {
                                    let code = close.map(|c| u16::from(c.code)).unwrap_or_default();
                                    ha.record(format!("close:{code}"));
                                    return;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            });
        }
    });
    Url::parse(&format!("ws://{addr}/api/websocket")).unwrap()
}

fn text(msg: Value) -> WsFrame {
    WsFrame::Text(msg.to_string().into())
}

impl FakeHa {
    /// Home Assistant's answers to a client message.
    fn reply(&self, msg: &Value) -> Vec<Value> {
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or_default();
        match msg_type {
            "auth" => {
                let token = msg
                    .get("access_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                self.record(format!("auth:{token}"));
                if self.reject_auth.load(Ordering::SeqCst) || token != TOKEN {
                    vec![json!({"type": "auth_invalid", "message": "Invalid access token"})]
                } else {
                    vec![json!({"type": "auth_ok", "ha_version": "2026.1.0"})]
                }
            }
            "ping" => {
                self.record("msg:ping");
                if self.ignore_pings.load(Ordering::SeqCst) {
                    Vec::new()
                } else {
                    vec![json!({"id": id, "type": "pong"})]
                }
            }
            "subscribe_events" => {
                self.record("msg:subscribe_events");
                vec![json!({"id": id, "type": "result", "success": true, "result": null})]
            }
            // The custom Unfolded Circle component is not installed, and everything else is
            // not required to get connected.
            other => {
                self.record(format!("msg:{other}"));
                vec![json!({
                    "id": id,
                    "type": "result",
                    "success": false,
                    "error": {"code": "unknown_command", "message": "Unknown command."}
                })]
            }
        }
    }
}

/// The Remote: records the `device_state` events it receives from the integration.
struct RemoteMock {
    states: Arc<Mutex<Vec<String>>>,
}

impl Actor for RemoteMock {
    type Context = Context<Self>;
}

impl Handler<SendWsMessage> for RemoteMock {
    type Result = ();

    fn handle(&mut self, msg: SendWsMessage, _ctx: &mut Self::Context) {
        if msg.0.msg.as_deref() == Some("device_state")
            && let Some(state) = msg
                .0
                .msg_data
                .as_ref()
                .and_then(|d| d.get("state"))
                .and_then(|s| s.as_str())
        {
            self.states.lock().unwrap().push(state.to_string());
        }
    }
}

struct Harness {
    ha: Arc<FakeHa>,
    controller: Addr<Controller>,
    states: Arc<Mutex<Vec<String>>>,
}

const WS_ID: &str = "remote";

impl Harness {
    /// Starts the fake Home Assistant server and a controller with a connected Remote session.
    async fn start(configure: impl FnOnce(&mut Settings)) -> Self {
        let ha = Arc::new(FakeHa::default());
        let url = spawn_fake_ha(ha.clone()).await;

        let mut settings = Settings::default();
        settings.hass.set_url(url);
        settings.hass.set_token(TOKEN);
        settings.hass.reconnect.duration = Duration::from_millis(100);
        settings.hass.reconnect.duration_max = Duration::from_millis(200);
        configure(&mut settings);

        let controller = Controller::new(settings, get_driver_metadata().unwrap()).start();
        let states = Arc::new(Mutex::new(Vec::new()));
        let remote = RemoteMock {
            states: states.clone(),
        }
        .start();
        controller
            .send(NewR2Session {
                addr: remote.recipient(),
                id: WS_ID.to_string(),
            })
            .await
            .unwrap();

        Self {
            ha,
            controller,
            states,
        }
    }

    async fn remote_event(&self, event: R2Event) {
        self.controller
            .send(R2EventMsg {
                ws_id: WS_ID.to_string(),
                event,
                msg_data: None,
            })
            .await
            .unwrap();
    }

    fn last_state(&self) -> Option<String> {
        self.states.lock().unwrap().last().cloned()
    }

    fn count_state(&self, state: &str) -> usize {
        self.states
            .lock()
            .unwrap()
            .iter()
            .filter(|s| *s == state)
            .count()
    }

    async fn wait_for_state(&self, state: &str) {
        wait_until(
            || self.last_state().as_deref() == Some(state),
            &format!(
                "device state {state}, states: {:?}",
                self.states.lock().unwrap()
            ),
        )
        .await;
    }
}

async fn wait_until(cond: impl Fn() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[actix_web::test]
async fn connect_authenticates_and_reports_connected() {
    let h = Harness::start(|_| {}).await;

    h.remote_event(R2Event::Connect).await;

    h.wait_for_state("CONNECTED").await;
    assert_eq!(h.ha.count("connect"), 1);
    assert_eq!(h.ha.count(&format!("auth:{TOKEN}")), 1);
    assert_eq!(h.ha.count("msg:subscribe_events"), 1);
    assert!(h.count_state("CONNECTING") >= 1);
}

#[actix_web::test]
async fn second_connect_request_does_not_open_a_second_connection() {
    let h = Harness::start(|_| {}).await;

    h.remote_event(R2Event::Connect).await;
    h.remote_event(R2Event::Connect).await;
    h.wait_for_state("CONNECTED").await;
    h.remote_event(R2Event::Connect).await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(h.ha.count("connect"), 1);
}

#[actix_web::test]
async fn lost_connection_is_reconnected_with_a_new_session() {
    let h = Harness::start(|_| {}).await;
    h.remote_event(R2Event::Connect).await;
    h.wait_for_state("CONNECTED").await;

    h.ha.drop_connection.notify_one();

    wait_until(|| h.count_state("CONNECTED") == 2, "reconnected").await;
    // the Remote was told about the interruption
    let states = h.states.lock().unwrap().clone();
    let first_connected = states.iter().position(|s| s == "CONNECTED").unwrap();
    assert_eq!(states[first_connected + 1], "CONNECTING");
    // the new connection authenticated and subscribed again
    assert_eq!(h.ha.count("connect"), 2);
    assert_eq!(h.ha.count(&format!("auth:{TOKEN}")), 2);
    assert_eq!(h.ha.count("msg:subscribe_events"), 2);
}

#[actix_web::test]
async fn unreachable_server_is_retried_until_it_is_back() {
    let h = Harness::start(|_| {}).await;
    h.ha.refuse_connections.store(true, Ordering::SeqCst);

    h.remote_event(R2Event::Connect).await;
    wait_until(|| h.ha.count("refused") >= 3, "connection attempts").await;
    assert_eq!(h.last_state().as_deref(), Some("CONNECTING"));

    h.ha.refuse_connections.store(false, Ordering::SeqCst);
    h.wait_for_state("CONNECTED").await;
}

#[actix_web::test]
async fn reconnect_attempt_limit_ends_in_error_state() {
    let h = Harness::start(|settings| settings.hass.reconnect.attempts = 2).await;
    h.ha.refuse_connections.store(true, Ordering::SeqCst);

    h.remote_event(R2Event::Connect).await;

    h.wait_for_state("ERROR").await;
    // the first attempt plus two reconnect attempts
    assert_eq!(h.ha.count("refused"), 3);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(h.ha.count("refused"), 3);

    // a new connect request of the Remote starts over
    h.ha.refuse_connections.store(false, Ordering::SeqCst);
    h.remote_event(R2Event::Connect).await;
    h.wait_for_state("CONNECTED").await;
}

#[actix_web::test]
async fn invalid_token_ends_in_error_state_without_reconnecting() {
    let h = Harness::start(|_| {}).await;
    h.ha.reject_auth.store(true, Ordering::SeqCst);

    h.remote_event(R2Event::Connect).await;

    h.wait_for_state("ERROR").await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(h.ha.count("connect"), 1);
    assert_eq!(h.last_state().as_deref(), Some("ERROR"));
    // the access token is not retried
    assert_eq!(h.ha.count(&format!("auth:{TOKEN}")), 1);
}

#[actix_web::test]
async fn invalid_message_closes_the_session_and_reconnects() {
    let h = Harness::start(|_| {}).await;
    h.ha.send_garbage.store(true, Ordering::SeqCst);

    h.remote_event(R2Event::Connect).await;

    // close code 1007: invalid payload data
    wait_until(
        || h.ha.count("close:1007") == 1,
        "close frame at the server",
    )
    .await;
    h.wait_for_state("CONNECTED").await;
    assert_eq!(h.ha.count("connect"), 2);
}

#[actix_web::test]
async fn disconnect_request_closes_the_connection_for_good() {
    let h = Harness::start(|_| {}).await;
    h.remote_event(R2Event::Connect).await;
    h.wait_for_state("CONNECTED").await;

    h.remote_event(R2Event::Disconnect).await;

    h.wait_for_state("DISCONNECTED").await;
    wait_until(
        || h.ha.count("close:1000") == 1,
        "close frame at the server",
    )
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(h.ha.count("connect"), 1);
    assert_eq!(h.last_state().as_deref(), Some("DISCONNECTED"));

    // connecting again works with a new client
    h.remote_event(R2Event::Connect).await;
    h.wait_for_state("CONNECTED").await;
    assert_eq!(h.ha.count("connect"), 2);
}

#[actix_web::test]
async fn disconnect_request_stops_a_running_reconnection() {
    let h = Harness::start(|_| {}).await;
    h.ha.refuse_connections.store(true, Ordering::SeqCst);
    h.remote_event(R2Event::Connect).await;
    wait_until(|| h.ha.count("refused") >= 2, "connection attempts").await;

    h.remote_event(R2Event::Disconnect).await;
    h.wait_for_state("DISCONNECTED").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let attempts = h.ha.count("refused");

    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(h.ha.count("refused"), attempts);
    assert_eq!(h.last_state().as_deref(), Some("DISCONNECTED"));
}

#[actix_web::test]
async fn standby_disconnects_and_wakeup_reconnects() {
    let h = Harness::start(|settings| settings.hass.disconnect_in_standby = true).await;
    h.remote_event(R2Event::Connect).await;
    h.wait_for_state("CONNECTED").await;

    h.remote_event(R2Event::EnterStandby).await;
    wait_until(
        || h.ha.count("close:1000") == 1,
        "close frame at the server",
    )
    .await;

    h.remote_event(R2Event::ExitStandby).await;
    wait_until(|| h.ha.count("connect") == 2, "second connection").await;
    wait_until(
        || h.ha.count("msg:subscribe_events") == 2,
        "subscribed again",
    )
    .await;
}

#[actix_web::test]
async fn api_heartbeat_pings_are_sent_and_keep_the_connection() {
    let h = Harness::start(|settings| {
        settings.hass.heartbeat.ping_frames = false;
        settings.hass.heartbeat.interval = Duration::from_millis(100);
        settings.hass.heartbeat.timeout = Duration::from_millis(300);
    })
    .await;
    h.remote_event(R2Event::Connect).await;
    h.wait_for_state("CONNECTED").await;

    wait_until(|| h.ha.count("msg:ping") >= 6, "heartbeat pings").await;

    assert_eq!(h.ha.count("connect"), 1);
    assert_eq!(h.last_state().as_deref(), Some("CONNECTED"));
}

#[actix_web::test]
async fn unanswered_api_heartbeat_reconnects() {
    let h = Harness::start(|settings| {
        settings.hass.heartbeat.ping_frames = false;
        settings.hass.heartbeat.interval = Duration::from_millis(100);
        settings.hass.heartbeat.timeout = Duration::from_millis(300);
    })
    .await;
    h.ha.ignore_pings.store(true, Ordering::SeqCst);
    h.remote_event(R2Event::Connect).await;
    h.wait_for_state("CONNECTED").await;

    wait_until(
        || h.ha.count("connect") >= 2,
        "reconnect after heartbeat timeout",
    )
    .await;
    assert!(h.count_state("CONNECTING") >= 2);
}

#[actix_web::test]
async fn ping_frame_heartbeat_keeps_the_connection_without_api_pings() {
    let h = Harness::start(|settings| {
        settings.hass.heartbeat.ping_frames = true;
        settings.hass.heartbeat.interval = Duration::from_millis(100);
        settings.hass.heartbeat.timeout = Duration::from_millis(300);
    })
    .await;
    h.remote_event(R2Event::Connect).await;
    h.wait_for_state("CONNECTED").await;

    tokio::time::sleep(Duration::from_millis(800)).await;

    // the WebSocket library of the fake server answers ping frames
    assert_eq!(h.ha.count("connect"), 1);
    assert_eq!(h.ha.count("msg:ping"), 0);
    assert_eq!(h.last_state().as_deref(), Some("CONNECTED"));
}
