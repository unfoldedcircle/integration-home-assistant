// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Custom application error with conversions from common Rust and 3rd-party errors.

use actix::dev::SendError;
use actix::MailboxError;
use derive_more::Display;
use log::error;
use std::io::ErrorKind;

#[derive(Debug, Display, PartialEq)]
pub enum ServiceError {
    #[display(fmt = "Internal server error")]
    InternalServerError(String),

    #[display(fmt = "Internal serialization error")]
    SerializationError(String),

    #[display(fmt = "BadRequest: {}", _0)]
    BadRequest(String),

    #[display(fmt = "Not found: {}", _0)]
    NotFound(String),

    #[display(fmt = "The connection is closed or closing")]
    NotConnected,

    ServiceUnavailable(String),
    NotYetImplemented,
}

impl From<std::io::Error> for ServiceError {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            ErrorKind::NotFound => ServiceError::NotFound(error.to_string()),
            // ErrorKind::PermissionDenied => ServiceError::AuthError(error.to_string()),
            ErrorKind::AlreadyExists | ErrorKind::InvalidInput | ErrorKind::InvalidData => {
                ServiceError::BadRequest(error.to_string())
            }

            ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::NotConnected
            | ErrorKind::AddrInUse
            | ErrorKind::AddrNotAvailable
            | ErrorKind::TimedOut => {
                error!("Connection error: {error:?}");
                ServiceError::ServiceUnavailable(error.to_string())
            }
            ErrorKind::BrokenPipe
            | ErrorKind::WouldBlock
            | ErrorKind::WriteZero
            | ErrorKind::Interrupted
            | ErrorKind::Unsupported
            | ErrorKind::UnexpectedEof
            | ErrorKind::OutOfMemory
            | ErrorKind::Other => {
                error!("Internal error: {:?}", error);
                ServiceError::InternalServerError(format!("{error:?}"))
            }
            _ => {
                error!("Other error: {:?}", error);
                ServiceError::InternalServerError(format!("{error:?}"))
            }
        }
    }
}

impl From<MailboxError> for ServiceError {
    fn from(e: MailboxError) -> Self {
        ServiceError::InternalServerError(format!("Internal message error: {:?}", e))
    }
}

impl From<serde_json::Error> for ServiceError {
    fn from(e: serde_json::Error) -> Self {
        error!("{:?}", e);
        ServiceError::SerializationError(e.to_string())
    }
}

impl From<strum::ParseError> for ServiceError {
    fn from(e: strum::ParseError) -> Self {
        ServiceError::SerializationError(e.to_string())
    }
}

impl<T> From<SendError<T>> for ServiceError {
    fn from(e: SendError<T>) -> Self {
        ServiceError::InternalServerError(format!("Error sending internal message: {:?}", e))
    }
}
