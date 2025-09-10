// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use rustls::{ServerConfig, pki_types::PrivateKeyDer};
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::ffi::OsStr;
use std::io::{BufReader, ErrorKind};
use std::path::Path;
use std::{fs, io};

/// Create a [`rustls::ServerConfig`] from the given public & private certificates.
///
/// # Arguments
///
/// * `cert_file`: path to public key file
/// * `key_file`: path to private key file (PKCS8-encoded)
///
/// returns: Result<ServerConfig, Error>
pub fn create_single_cert_server_config<S: AsRef<OsStr> + ?Sized>(
    cert_file: &S,
    key_file: &S,
) -> Result<ServerConfig, io::Error> {
    let cert_file = Path::new(cert_file);
    let key_file = Path::new(key_file);

    if !(cert_file.exists() && key_file.exists()) {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            format!("Custom certificates not found: {cert_file:?}, {key_file:?}"),
        ));
    }

    let f = fs::File::open(cert_file)?;
    let mut reader = BufReader::new(f);

    let cert_chain = certs(&mut reader).collect::<Result<Vec<_>, _>>()?;

    let f = fs::File::open(key_file)?;
    let mut reader = BufReader::new(f);
    let mut keys = pkcs8_private_keys(&mut reader).collect::<Result<Vec<_>, _>>()?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, PrivateKeyDer::Pkcs8(keys.remove(0)))
        .expect("bad certificate/key");

    Ok(config)
}

#[cfg(test)]
mod tests {

    #[test]
    fn load_ssl_with_invalid_cert_paths_returns_error() {
        let result = super::create_single_cert_server_config("invalid", "invalid");
        assert!(
            result.is_err(),
            "load_ssl must return an error with invalid cert paths"
        );
    }
}
