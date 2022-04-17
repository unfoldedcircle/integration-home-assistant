// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

#![forbid(non_ascii_idents)]
#![deny(unsafe_code)]

use std::io;
use std::net::TcpListener;

use actix::Actor;
use actix_web::{middleware, web, App, HttpServer};

use server::json_error_handler;

use crate::configuration::get_configuration;
use crate::controller::Controller;

mod client;
mod configuration;
mod controller;
mod errors;
mod from_msg_data;
mod messages;
mod server;
mod websocket;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cfg = get_configuration().expect("Failed to read configuration");

    let address = format!("{}:{}", cfg.webserver.interface, cfg.webserver.http_port);
    let listener = Some(TcpListener::bind(address)?);
    let listener_tls = if cfg.webserver.https {
        let address = format!("{}:{}", cfg.webserver.interface, cfg.webserver.https_port);
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

    let websocket_settings = web::Data::new(cfg.webserver.websocket.unwrap_or_default());
    let controller = web::Data::new(Controller::new(cfg.home_assistant).start());

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

    // if listener_tls.is_some() {
    //     let server_cfg = load_ssl(&settings.webserver.certs)?;
    //     http_server = http_server.listen_rustls(listener_tls.unwrap(), server_cfg)?;
    // }

    if listener.is_some() {
        http_server = http_server.listen(listener.unwrap())?;
    }

    http_server.run().await?;

    Ok(())
}
