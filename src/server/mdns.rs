// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! mDNS advertisement with mdns-sd Rust crate

use crate::errors::ServiceError;
use crate::util::my_ipv4_interfaces;
use lazy_static::lazy_static;
use log::{error, info};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::net::Ipv4Addr;

lazy_static! {
    pub static ref MDNS_SERVICE: Option<ServiceDaemon> = match ServiceDaemon::new() {
        Ok(s) => Some(s),
        Err(e) => {
            error!(
                "Failed to create mdns daemon, service publishing won't be available! Error: {e}"
            );
            None
        }
    };
}

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
    instance_name: impl AsRef<str>,
    service_name: impl AsRef<str>,
    protocol: impl AsRef<str>,
    port: u16,
    txt: Vec<String>,
) -> Result<(), ServiceError> {
    if let Some(mdns_service) = &*MDNS_SERVICE {
        let mut reg_type = format!("_{}._{}", service_name.as_ref(), protocol.as_ref());
        if !reg_type.ends_with(".local.") {
            reg_type.push_str(".local.");
        }
        let my_addrs: Vec<Ipv4Addr> = my_ipv4_interfaces().iter().map(|i| i.ip).collect();
        let hostname = hostname::get().map(|name| name.to_string_lossy().to_string())?;

        let properties = txt
            .iter()
            .filter_map(|v| v.split_once('='))
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        if let Err(e) = ServiceInfo::new(
            &reg_type,
            instance_name.as_ref(),
            &hostname,
            &my_addrs[..],
            port,
            Some(properties),
        )
        .and_then(|service_info| {
            let fullname = service_info.get_fullname().to_string();
            mdns_service.register(service_info)?;
            info!("Registered service: {fullname}");
            Ok(())
        }) {
            Err(ServiceError::InternalServerError(format!(
                "Failed to register {reg_type} mdns service! Error: {e}"
            )))
        } else {
            Ok(())
        }
    } else {
        Err(ServiceError::ServiceUnavailable(
            "mDNS service not available".into(),
        ))
    }
}
