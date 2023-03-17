// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use crate::util::bool_from_env;
use actix_tls::connect::rustls::webpki_roots_cert_store;
use if_addrs::{IfAddr, Ifv4Addr};
use rustls::ClientConfig;
use std::sync::Arc;
use std::time::Duration;

pub fn my_ipv4_interfaces() -> Vec<Ifv4Addr> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| {
            if i.is_loopback() {
                None
            } else {
                match i.addr {
                    IfAddr::V4(ifv4) => Some(ifv4),
                    _ => None,
                }
            }
        })
        .collect()
}

pub fn new_websocket_client(connection_timeout: Duration, tls: bool) -> awc::Client {
    if tls {
        // TLS configuration: https://github.com/actix/actix-web/blob/master/awc/tests/test_rustls_client.rs
        // TODO self-signed certificate handling
        let mut config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(webpki_roots_cert_store())
            .with_no_client_auth();

        // http2 has (or at least had) issues with wss. Needs further investigation.
        config.alpn_protocols = vec![b"http/1.1".to_vec()];

        // Disable TLS verification
        // Requires: rustls = { ... optional = true, features = ["dangerous_configuration"] }
        if bool_from_env("UC_DISABLE_CERT_VERIFICATION") {
            config
                .dangerous()
                .set_certificate_verifier(Arc::new(danger::NoCertificateVerification {}));
        }

        let connector = awc::Connector::new().rustls(Arc::new(config));
        awc::ClientBuilder::new()
            .timeout(connection_timeout)
            .connector(connector)
            .finish()
    } else {
        awc::ClientBuilder::new()
            .timeout(connection_timeout)
            .finish()
    }
}

mod danger {
    use rustls::client::{ServerCertVerified, ServerCertVerifier};
    use std::time::SystemTime;

    pub struct NoCertificateVerification {}

    impl ServerCertVerifier for NoCertificateVerification {
        fn verify_server_cert(
            &self,
            _end_entity: &rustls::Certificate,
            _intermediates: &[rustls::Certificate],
            _server_name: &rustls::ServerName,
            _scts: &mut dyn Iterator<Item = &[u8]>,
            _ocsp_response: &[u8],
            _now: SystemTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }
    }
}
