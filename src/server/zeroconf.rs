// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! mDNS advertisement with Zeroconf (Avahi or Bonjour)

use crate::errors::ServiceError;

use log::{error, info};
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;
use zeroconf::prelude::*;
use zeroconf::{MdnsService, ServiceRegistration, ServiceType, TxtRecord};

/// Publish a service on all available network interfaces with the default hostname.
///
/// # Arguments
///
/// * `instance_name`: Instance name
/// * `service_name`: The service name (e.g. `http`).
/// * `protocol`: The protocol of the service (e.g. `tcp`).
/// * `port`: The port on which the service accepts connections.
/// * `txt`: Optional TXT record data with format: `key=value`. The value is optional.
pub fn publish_service(
    instance_name: impl ToString,
    service_name: impl AsRef<str>,
    protocol: impl AsRef<str>,
    port: u16,
    txt: Vec<String>,
) -> Result<(), ServiceError> {
    let instance_name = instance_name.to_string();
    let service = ServiceType::new(service_name.as_ref(), protocol.as_ref())
        .map_err(|e| ServiceError::BadRequest(e.to_string()))?;
    std::thread::spawn(move || service_publisher(instance_name, service, port, txt));

    Ok(())
}

/// Publisher thread polling the event loop.
fn service_publisher(
    instance_name: String,
    service_type: ServiceType,
    port: u16,
    txt: Vec<String>,
) {
    let mut service = MdnsService::new(service_type, port);
    let mut txt_record = TxtRecord::new();

    for record in txt {
        if let Some((key, value)) = record.split_once('=') {
            txt_record.insert(key, value).unwrap();
        }
    }

    service.set_name(instance_name.as_ref());
    service.set_registered_callback(Box::new(on_service_registered));
    service.set_txt_record(txt_record);

    let event_loop = match service.register() {
        Ok(el) => el,
        Err(e) => {
            error!("Failed to register service! Error: {e}");
            return;
        }
    };

    loop {
        // What is a good production timeout?
        if let Err(e) = event_loop.poll(Duration::from_secs(1)) {
            error!("mDNS event loop polling error: {e}");
            return;
        }
    }
}

fn on_service_registered(
    result: zeroconf::Result<ServiceRegistration>,
    _context: Option<Arc<dyn Any>>,
) {
    match result {
        Ok(r) => info!("{:?}", r),
        Err(e) => error!("Service registration error: {e}"),
    }
}
