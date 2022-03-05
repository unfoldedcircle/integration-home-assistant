// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use actix_web::error::JsonPayloadError;
use actix_web::{error, Error, HttpRequest, HttpResponse};
use serde::Serialize;

/// Rest API response
#[derive(Debug, Serialize)]
pub struct ApiResponse<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<&'a str>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<&'a str>,
}

impl<'a> ApiResponse<'a> {
    pub fn new(code: &'a str, message: &'a str) -> ApiResponse<'a> {
        ApiResponse {
            code: Some(code),
            message: Some(message),
        }
    }
}

pub fn json_error_handler(err: error::JsonPayloadError, _: &HttpRequest) -> Error {
    let message = err.to_string();

    let resp = match &err {
        JsonPayloadError::ContentType => HttpResponse::UnsupportedMediaType()
            .json(ApiResponse::new("UNSUPPORTED_MEDIA_TYPE", &message[..])),
        JsonPayloadError::Deserialize(json_err) if json_err.is_data() => {
            // alternative: HttpResponse::UnprocessableEntity 422
            HttpResponse::BadRequest().json(ApiResponse::new("INVALID_JSON", &message[..]))
        }
        _ => HttpResponse::BadRequest().json(ApiResponse::new("BAD_REQUEST", &message[..])),
    };

    error::InternalError::from_response(err, resp).into()
}
