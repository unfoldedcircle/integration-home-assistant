// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use crate::configuration::ENV_DISABLE_CERT_VERIFICATION;
use crate::util::bool_from_env;
use rustls::ClientConfig;
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "mdns-sd")]
pub fn my_ipv4_interfaces() -> Vec<if_addrs::IfAddr> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| {
            if i.is_loopback() {
                None
            } else {
                match i.addr {
                    if_addrs::IfAddr::V4(_) => Some(i.addr),
                    _ => None,
                }
            }
        })
        .collect()
}

pub fn new_websocket_client(
    connection_timeout: Duration,
    request_timeout: Duration,
    disable_cert_validation: bool,
) -> awc::Client {
    use rustls_platform_verifier::ConfigVerifierExt as _;

    // TLS configuration: https://github.com/actix/actix-web/blob/master/awc/tests/test_rustls_client.rs
    // TODO self-signed certificate handling #4
    let mut config =
        ClientConfig::with_platform_verifier().expect("Platform certificate verifier required");

    // http2 has (or at least had) issues with wss. Needs further investigation.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];

    // Disable TLS verification
    if disable_cert_validation || bool_from_env(ENV_DISABLE_CERT_VERIFICATION) {
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(danger::NoCertificateVerification {}));
    }

    awc::Client::builder()
        .timeout(request_timeout)
        .connector(
            awc::Connector::new()
                .rustls_0_23(Arc::new(config))
                .timeout(connection_timeout),
        )
        .finish()
}

mod danger {
    use log::warn;
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{DigitallySignedStruct, Error, SignatureScheme};
    use std::fmt::Debug;

    #[derive(Debug)]
    pub struct NoCertificateVerification {}

    impl ServerCertVerifier for NoCertificateVerification {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            warn!("Certificate verification disabled");
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            rustls::crypto::aws_lc_rs::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }
}
