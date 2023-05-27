// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Server modules of the integration driver. Handling WebSocket and mDNS advertisement.

// zeroconf has priority over mdns-sd
#[cfg(feature = "zeroconf")]
mod zeroconf;
#[cfg(feature = "zeroconf")]
pub use self::zeroconf::publish_service;

#[cfg(feature = "mdns-sd")]
mod mdns;
#[cfg(feature = "mdns-sd")]
#[cfg(not(feature = "zeroconf"))]
pub use mdns::publish_service;

mod ws;
pub use ws::{json_error_handler, ws_index};

/// Fallback if no mDNS library is enabled
#[cfg(not(feature = "zeroconf"))]
#[cfg(not(feature = "mdns-sd"))]
pub fn publish_service(
    _instance_name: impl AsRef<str>,
    _reg_type: impl Into<String>,
    _port: u16,
    _txt: Vec<String>,
) -> Result<(), crate::errors::ServiceError> {
    log::warn!("No mDNS library support included: service will not be published!");
    Ok(())
}
