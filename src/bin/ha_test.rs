// Copyright (c) 2024 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home-Assistant WebSocket API connection test tool

use actix::{Actor, Addr, Context, Handler};
use actix_web::rt::time::sleep;
use clap::{Arg, Command};
use log::{debug, error, info};
use std::env;
use std::str::FromStr;
use std::time::Duration;
use uc_api::intg::ws::{R2Event, R2Request};
use uc_intg_hass::configuration::{get_configuration, Settings, DEF_HA_URL, ENV_HASS_MSG_TRACING};
use uc_intg_hass::{
    configuration, Controller, NewR2Session, R2EventMsg, R2RequestMsg, SendWsMessage, APP_VERSION,
};
use url::Url;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let cfg = parse_args_load_cfg()?;

    let driver_metadata = configuration::get_driver_metadata()?;
    let controller = Controller::new(cfg, driver_metadata.clone()).start();

    // Mock server to simulate an R2 connection
    let ws_id = "HA-test".to_string();
    let server = ServerMock::new(&ws_id, controller.clone()).start();

    // establish a mock session
    controller
        .send(NewR2Session {
            addr: server.recipient(),
            id: ws_id.clone(),
        })
        .await?;

    // connect to HA
    controller
        .send(R2EventMsg {
            ws_id: ws_id.clone(),
            event: R2Event::Connect,
            msg_data: None,
        })
        .await?;

    // quick and dirty for now
    sleep(Duration::from_secs(30)).await;

    Ok(())
}

fn parse_args_load_cfg() -> anyhow::Result<Settings> {
    let args = Command::new("ha-test")
        .author("Unfolded Circle ApS")
        .version(APP_VERSION)
        .about("Home Assistant server communication test")
        .arg(
            Arg::new("url")
                .short('u')
                .default_value(DEF_HA_URL)
                .help("Home Assistant WebSocket API URL (overrides home-assistant.json)"),
        )
        .arg(
            Arg::new("token")
                .short('t')
                .help("Home Assistant long lived access token (overrides home-assistant.json)"),
        )
        .arg(
            Arg::new("connection_timeout")
                .short('c')
                .help("TCP connection timeout in seconds (overrides home-assistant.json)"),
        )
        .arg(
            Arg::new("request_timeout")
                .short('r')
                .help("Request timeout in seconds (overrides home-assistant.json)"),
        )
        .arg(
            Arg::new("trace_level")
                .long("trace")
                .value_name("MESSAGES")
                .value_parser(["in", "out", "all", "none"])
                .default_value("all")
                .help("Message tracing for HA server communication"),
        )
        .get_matches();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    if let Some(msg_trace) = args.get_one::<String>("trace_level") {
        env::set_var(ENV_HASS_MSG_TRACING, msg_trace);
    }
    let cfg_file = None;
    let mut cfg = get_configuration(cfg_file).expect("Failed to read configuration");
    if let Some(url) = args.get_one::<String>("url") {
        cfg.hass.url = Url::parse(url)?;
    }
    if let Some(token) = args.get_one::<String>("token") {
        cfg.hass.token = token.clone();
    }
    if let Some(timeout) = args.get_one::<String>("connection_timeout") {
        cfg.hass.connection_timeout = u8::from_str(timeout)?;
    }
    if let Some(timeout) = args.get_one::<String>("request_timeout") {
        cfg.hass.request_timeout = u8::from_str(timeout)?;
    }

    if !cfg.hass.url.has_host() || cfg.hass.token.is_empty() {
        eprintln!("Can't connect to Home Assistant: URL or token is missing");
        std::process::exit(1);
    }

    Ok(cfg)
}

struct ServerMock {
    id: String,
    connected: bool,
    controller_addr: Addr<Controller>,
}

impl ServerMock {
    fn new(id: impl Into<String>, controller_addr: Addr<Controller>) -> Self {
        Self {
            id: id.into(),
            connected: false,
            controller_addr,
        }
    }
}
impl Actor for ServerMock {
    type Context = Context<Self>;
}

impl Handler<SendWsMessage> for ServerMock {
    type Result = ();

    fn handle(&mut self, msg: SendWsMessage, _ctx: &mut Self::Context) {
        let msg_name = msg.0.msg.clone().unwrap_or_default();
        if msg_name == "device_state" {
            if let Some(msg_data) = msg.0.msg_data.as_ref() {
                self.connected = msg_data
                    .as_object()
                    .and_then(|o| o.get("state"))
                    .and_then(|s| s.as_str())
                    == Some("CONNECTED");

                if self.connected {
                    self.controller_addr.do_send(R2RequestMsg {
                        ws_id: self.id.clone(),
                        req_id: 0,
                        request: R2Request::GetEntityStates,
                        msg_data: None,
                    });
                }
            }
        }

        if let Ok(msg) = serde_json::to_string(&msg.0) {
            if msg_name == "entity_states" {
                info!("[{}] <- entity_states:\n{msg}", self.id);
                self.controller_addr.do_send(R2EventMsg {
                    ws_id: self.id.clone(),
                    event: R2Event::Disconnect,
                    msg_data: None,
                });
                info!("Entity states received, disconnecting!");
            } else {
                debug!("[{}] <- {msg}", self.id);
            }
        } else {
            error!("[{}] Error serializing {:?}", self.id, msg.0)
        }
    }
}
