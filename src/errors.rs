// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use actix::dev::SendError;
use actix::MailboxError;
use derive_more::Display;
use log::error;
use serde_json::Error;
use strum::ParseError;

#[derive(Debug, Display, PartialEq)]
pub enum ServiceError {
    #[display(fmt = "Internal server error")]
    InternalServerError,

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
}

impl From<std::io::Error> for ServiceError {
    fn from(e: std::io::Error) -> Self {
        // TODO error conversion
        error!("{:?}", e);
        ServiceError::InternalServerError
    }
}

impl From<MailboxError> for ServiceError {
    fn from(e: MailboxError) -> Self {
        error!("{:?}", e);
        ServiceError::InternalServerError
    }
}

impl From<serde_json::Error> for ServiceError {
    fn from(e: Error) -> Self {
        error!("{:?}", e);
        ServiceError::SerializationError(e.to_string())
    }
}

impl From<strum::ParseError> for ServiceError {
    fn from(e: ParseError) -> Self {
        ServiceError::SerializationError(e.to_string())
    }
}

impl<T> From<actix::prelude::SendError<T>> for ServiceError {
    fn from(e: SendError<T>) -> Self {
        error!("{:?}", e);
        ServiceError::InternalServerError
    }
}
