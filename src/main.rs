// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

#![forbid(non_ascii_idents)]
#![deny(unsafe_code)]

use std::collections::HashMap;
use std::io;
use std::net::TcpListener;
use std::path::Path;

use actix::Actor;
use actix_web::{middleware, web, App, HttpServer};
use clap::{arg, Command};
use const_format::formatcp;
use lazy_static::lazy_static;
use log::{error, info};
use server::json_error_handler;
use uc_api::intg::IntegrationDriverUpdate;
use uc_api::util::text_from_language_map;

use crate::configuration::get_configuration;
use crate::controller::Controller;
use crate::server::publish_service;

mod client;
mod configuration;
mod controller;
mod errors;
mod from_msg_data;
mod messages;
mod server;
mod util;
mod websocket;

const DEF_CONFIG_FILE: &str = "configuration.yaml";
const DRIVER_METADATA: &str = include_str!("../resources/driver.json");

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

pub const APP_VERSION: &str = formatcp!(
    "{}{}",
    match built_info::GIT_VERSION {
        Some(v) => v,
        None => formatcp!("{}-non-git", built_info::PKG_VERSION),
    },
    match built_info::GIT_DIRTY {
        Some(_) => "-dirty",
        None => "",
    }
);

lazy_static! {
    pub static ref API_VERSION: &'static str = built_info::DEPENDENCIES
        .iter()
        .find(|p| p.0 == "uc_api")
        .map(|v| v.1)
        .unwrap_or("?");
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    let args = Command::new(built_info::PKG_NAME)
        .author("Unfolded Circle Aps")
        .version(APP_VERSION)
        .about("Home Assistant integration for Remote Two")
        .arg(arg!(-c --config <FILE> ... "Configuration file").required(false))
        .get_matches();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cfg_file = match args.value_of("config") {
        None => {
            if Path::new(DEF_CONFIG_FILE).exists() {
                info!("Loading default configuration file: {}", DEF_CONFIG_FILE);
                Some(DEF_CONFIG_FILE)
            } else {
                None
            }
        }
        Some(c) => Some(c),
    };
    let cfg = get_configuration(cfg_file).expect("Failed to read configuration");

    let driver_metadata = get_driver_metadata()?;

    let listener = if cfg.integration.http.enabled {
        let address = format!(
            "{}:{}",
            cfg.integration.interface, cfg.integration.http.port
        );
        println!("{} listening on: {}", built_info::PKG_NAME, address);
        Some(TcpListener::bind(address)?)
    } else {
        None
    };
    let listener_tls = if cfg.integration.https.enabled {
        let address = format!(
            "{}:{}",
            cfg.integration.interface, cfg.integration.https.port
        );
        println!("{} listening on: {}", built_info::PKG_NAME, address);
        Some(TcpListener::bind(address)?)
    } else {
        None
    };

    if listener.is_none() && listener_tls.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "At least one http or https listener must be specified",
        ));
    }

    let api_port = cfg.integration.http.port;
    let websocket_settings = web::Data::new(cfg.integration.websocket.clone().unwrap_or_default());
    let controller = web::Data::new(Controller::new(cfg, driver_metadata.clone()).start());

    let mut http_server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(
                web::JsonConfig::default()
                    .limit(16 * 1024) // limit size of the payload (global configuration)
                    .error_handler(json_error_handler),
            )
            .app_data(websocket_settings.clone())
            .app_data(controller.clone()) //register the lobby
            // Websockets
            .service(server::ws_index)
    })
    .workers(1);

    if listener_tls.is_some() {
        // let server_cfg = load_ssl(&settings.webserver.certs)?;
        //http_server = http_server.listen_rustls(listener_tls.unwrap(), server_cfg)?;
        error!("TODO certificate handling not yet implemented. Please use http only. Sorry.");
        std::process::exit(1);
    }

    if listener.is_some() {
        http_server = http_server.listen(listener.unwrap())?;
    }

    publish_mdns(api_port, driver_metadata);

    http_server.run().await?;

    Ok(())
}

fn get_driver_metadata() -> Result<IntegrationDriverUpdate, io::Error> {
    let mut driver: IntegrationDriverUpdate =
        serde_json::from_str(DRIVER_METADATA).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid driver.json format: {e}"),
            )
        })?;

    if driver.driver_id.is_none() {
        driver.driver_id = Some("home-assistant".into())
    }
    if !driver
        .name
        .as_ref()
        .map(|v| !v.is_empty())
        .unwrap_or_default()
    {
        driver.name = Some(HashMap::from([("en".into(), "Home Assistant".into())]))
    }
    driver.token = None; // don't expose sensitive information
    driver.version = Some(APP_VERSION.to_string());

    Ok(driver)
}

fn publish_mdns(api_port: u16, drv_metadata: IntegrationDriverUpdate) {
    if let Err(e) = publish_service(
        drv_metadata
            .driver_id
            .expect("driver_id must be set in driver metadata"),
        "_uc-integration._tcp",
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
                    .unwrap_or("Unfolded Circle Aps".into())
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
