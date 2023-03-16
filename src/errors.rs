// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Custom application error with conversions from common Rust and 3rd-party errors.

use actix::dev::SendError;
use actix::MailboxError;
use derive_more::Display;
use log::error;

#[derive(Debug, Display, PartialEq)]
pub enum ServiceError {
    #[display(fmt = "Internal server error")]
    InternalServerError(String),

    #[display(fmt = "Internal serialization error")]
    SerializationError(String),

    #[display(fmt = "BadRequest: {}", _0)]
    BadRequest(String),

    // #[display(fmt = "Validation error: {}", _0)]
    // ValidationError(String),
    //
    // #[display(fmt = "No information found")]
    // NotFound,
    //
    // #[display(fmt = "Data already exists")]
    // AlreadyExists(String),
    #[display(fmt = "The connection is closed or closing")]
    NotConnected,

    NotYetImplemented,
    // AuthError(String),
    ServiceUnavailable(String),
}

impl From<std::io::Error> for ServiceError {
    fn from(e: std::io::Error) -> Self {
        // TODO error conversion
        ServiceError::InternalServerError(format!("{:?}", e))
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
