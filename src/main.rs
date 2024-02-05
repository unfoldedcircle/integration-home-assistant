// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home-Assistant Integration for Remote Two
//!
//! This service application connects [`Home Assistant`](https://www.home-assistant.io/) with the
//! [`Remote Two`](https://www.unfoldedcircle.com/) and allows to interact with most entities on
//! the remote.  
//! It implements the Remote Two [`Integration-API`](https://github.com/unfoldedcircle/core-api)
//! which communicates with JSON messages over WebSocket.
//!
//! The WebSocket server and client uses [`Actix Web`](https://actix.rs/) with the Actix actor
//! system for internal service communication.

#![forbid(non_ascii_idents)]
#![deny(unsafe_code)]

use crate::configuration::{
    get_configuration, CertificateSettings, IntegrationSettings, ENV_DISABLE_MDNS_PUBLISH,
};
use crate::controller::Controller;
use crate::server::publish_service;
use crate::util::{bool_from_env, create_single_cert_server_config};
use actix::Actor;
use actix_web::{middleware, web, App, HttpServer};
use clap::{arg, Command};
use configuration::DEF_CONFIG_FILE;
use log::{error, info};
use std::io;
use std::net::TcpListener;
use std::path::Path;
use uc_api::intg::IntegrationDriverUpdate;
use uc_api::util::text_from_language_map;
use uc_intg_hass::{built_info, APP_VERSION};

mod client;
mod configuration;
mod controller;
mod errors;
mod server;
mod util;

#[actix_web::main]
async fn main() -> io::Result<()> {
    let args = Command::new(built_info::PKG_NAME)
        .author("Unfolded Circle ApS")
        .version(APP_VERSION)
        .about("Home Assistant integration for Remote Two")
        .arg(arg!(-c --config <FILE> ... "Configuration file").required(false))
        .get_matches();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cfg_file: Option<&str> =
        args.get_one("config")
            .map(|c: &String| c.as_str())
            .or_else(|| {
                if Path::new(DEF_CONFIG_FILE).exists() {
                    info!("Loading default configuration file: {}", DEF_CONFIG_FILE);
                    Some(DEF_CONFIG_FILE)
                } else {
                    None
                }
            });

    let cfg = get_configuration(cfg_file).expect("Failed to read configuration");

    let listeners = create_tcp_listeners(&cfg.integration)?;
    let api_port = cfg.integration.http.port;
    let websocket_settings = web::Data::new(cfg.integration.websocket.clone().unwrap_or_default());
    let driver_metadata = configuration::get_driver_metadata()?;

    let controller = web::Data::new(Controller::new(cfg, driver_metadata.clone()).start());

    let mut http_server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(
                web::JsonConfig::default()
                    .limit(16 * 1024) // limit size of the payload (global configuration)
                    .error_handler(server::json_error_handler),
            )
            .app_data(websocket_settings.clone())
            .app_data(controller.clone())
            // Websockets
            .service(server::ws_index)
    })
    .workers(1);

    if let Some(listener) = listeners.listener_tls {
        let server_cfg =
            create_single_cert_server_config(&listeners.certs.public, &listeners.certs.private)?;
        http_server = http_server.listen_rustls_0_21(listener, server_cfg)?;
    }

    if let Some(listener) = listeners.listener {
        http_server = http_server.listen(listener)?;
    }

    if !bool_from_env(ENV_DISABLE_MDNS_PUBLISH) {
        publish_mdns(api_port, driver_metadata);
    }

    http_server.run().await?;

    Ok(())
}

struct Listeners {
    pub listener: Option<TcpListener>,
    pub listener_tls: Option<TcpListener>,
    pub certs: CertificateSettings,
}

fn create_tcp_listeners(cfg: &IntegrationSettings) -> Result<Listeners, io::Error> {
    let listener = if cfg.http.enabled {
        let address = format!("{}:{}", cfg.interface, cfg.http.port);
        println!("{} listening on: {address}", built_info::PKG_NAME);
        Some(TcpListener::bind(address)?)
    } else {
        None
    };

    let (listener_tls, certs) = if cfg.https.enabled {
        let address = format!("{}:{}", cfg.interface, cfg.https.port);
        let certs = match cfg.certs.as_ref() {
            None => {
                error!("https requires integration.certs settings");
                std::process::exit(1);
            }
            Some(c) => c.clone(),
        };

        println!("{} listening on: {address}", built_info::PKG_NAME);
        (Some(TcpListener::bind(address)?), certs)
    } else {
        (None, Default::default())
    };

    if listener.is_none() && listener_tls.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "At least one http or https listener must be specified",
        ));
    }

    Ok(Listeners {
        listener,
        listener_tls,
        certs,
    })
}

/// Advertise integration driver with mDNS.
fn publish_mdns(api_port: u16, drv_metadata: IntegrationDriverUpdate) {
    if let Err(e) = publish_service(
        drv_metadata
            .driver_id
            .expect("driver_id must be set in driver metadata"),
        "uc-integration",
        "tcp",
        api_port,
        vec![
            format!(
                "name={}",
                text_from_language_map(drv_metadata.name.as_ref(), "en")
                    .unwrap_or("Home Assistant")
            ),
            format!(
                "developer={}",
                drv_metadata
                    .developer
                    .and_then(|d| d.name)
                    .unwrap_or("Unfolded Circle ApS".into())
            ),
            // "ws_url=wss://localhost:8008".into(), // to override the complete WS url. Ignores ws_path, wss, wss_port!
            "ws_path=/ws".into(), // otherwise `/` is used and the remote can't connect
            //"wss=false".into(), // if wss is required
            //format!("wss_port={}", cfg.integration.https.port), // if https port if different from the published service port above
            format!("pwd={}", drv_metadata.pwd_protected.unwrap_or_default()),
            format!("ver={APP_VERSION}"),
        ],
    ) {
        error!("Error publishing mDNS service: {e}");
    }
}
