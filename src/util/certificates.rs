// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use rustls::ServerConfig;
use std::ffi::OsStr;
use std::io::{BufReader, ErrorKind};
use std::path::Path;
use std::{fs, io};

/// Create a [`rustls::ServerConfig`] from the given public & private certificates.
///
/// # Arguments
///
/// * `cert_file`: path to public key file
/// * `key_file`: path to private key file containing either a DER-encoded plaintext RSA private key
///               (as specified in PKCS#1/RFC3447) or a DER-encoded plaintext private key (as
///               specified in PKCS#8/RFC5958).
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

    let cert_chain = load_certs(cert_file)?;
    let private_key = load_private_key(key_file)?;

    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .expect("bad certificate/key");

    Ok(config)
}

fn load_certs(filename: &Path) -> Result<Vec<rustls::Certificate>, io::Error> {
    let cert_file = fs::File::open(filename)?;
    let mut reader = BufReader::new(cert_file);
    Ok(rustls_pemfile::certs(&mut reader)?
        .iter()
        .map(|v| rustls::Certificate(v.clone()))
        .collect())
}

fn load_private_key(filename: &Path) -> Result<rustls::PrivateKey, io::Error> {
    let key_file = fs::File::open(filename)?;
    let mut reader = BufReader::new(key_file);

    load_private_key_from_reader(&mut reader)
}

fn load_private_key_from_reader(
    reader: &mut dyn io::BufRead,
) -> Result<rustls::PrivateKey, io::Error> {
    loop {
        match rustls_pemfile::read_one(reader)? {
            Some(rustls_pemfile::Item::RSAKey(key)) => return Ok(rustls::PrivateKey(key)),
            Some(rustls_pemfile::Item::PKCS8Key(key)) => return Ok(rustls::PrivateKey(key)),
            None => break,
            _ => {}
        }
    }

    Err(io::Error::new(
        ErrorKind::InvalidData,
        "No compatible private keys found (encrypted keys not supported)",
    ))
}

#[cfg(test)]
mod tests {
    use std::io::BufReader;
    use std::path::PathBuf;
    use std::{env, io};
    use uuid::Uuid;

    #[test]
    fn load_ssl_with_invalid_cert_paths_returns_error() {
        let result = super::create_single_cert_server_config("invalid", "invalid");
        assert!(
            result.is_err(),
            "load_ssl must return an error with invalid cert paths"
        );
    }

    #[test]
    fn load_certs_with_invalid_filename_returns_error() {
        let mut invalid_path = PathBuf::new();

        invalid_path.push(env::temp_dir());
        invalid_path.push(Uuid::new_v4().hyphenated().to_string());
        let result = super::load_certs(&invalid_path);
        assert!(
            result.is_err(),
            "load_certs must return an error for an invalid path"
        );
        assert_eq!(
            Err(io::ErrorKind::NotFound),
            result.map_err(|e| e.kind()),
            "Expected io::ErrorKind::NotFound for invalid input path"
        );
    }

    #[test]
    fn load_private_key_from_reader_with_empty_buffer_returns_invalid_data_error() {
        let empty_buffer: &[u8] = &[];
        let mut reader = BufReader::new(empty_buffer);
        let result = super::load_private_key_from_reader(&mut reader);
        assert!(result.is_err(), "Expected error for empty input buffer");
        assert_eq!(
            Err(io::ErrorKind::InvalidData),
            result.map_err(|e| e.kind()),
            "Expected io::ErrorKind::InvalidData for empty input buffer"
        );
    }
}
