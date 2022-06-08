// This file is part of SubQuery.

// Copyright (C) 2020-2022 SubQuery Pte Ltd authors & contributors
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use serde::Serialize;
use std::convert::Infallible;
use std::fmt;
use thiserror::Error;
use warp::{http::StatusCode, Rejection, Reply};

use crate::cli::COMMAND;

// TODO: reorganise the errors
#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid auth token")]
    JWTTokenError,
    #[error("invalid payload to create token")]
    JWTTokenCreationError,
    #[error("invalid auth header")]
    InvalidAuthHeaderError,
    #[error("permission deny")]
    NoPermissionError,
    #[error("token expired")]
    JWTTokenExpiredError,
    #[error("invalid project id")]
    InvalidProejctId,
    #[error("invalid coordinator service endpoint")]
    InvalidServiceEndpoint,
    #[error("invalid or missing controller")]
    InvalidController,
    #[error("invalid serialize")]
    InvalidSerialize,
    #[error("invalid signature")]
    InvalidSignature,
    #[error("invalid encrypt or decrypt")]
    InvalidEncrypt,
    #[error("service exception")]
    ServiceException,
}

#[derive(Serialize, Debug)]
struct ErrorResponse {
    message: String,
    status: String,
}

impl warp::reject::Reject for Error {}

pub async fn handle_rejection(err: Rejection) -> std::result::Result<impl Reply, Infallible> {
    let (code, message) = if err.is_not_found() {
        (StatusCode::NOT_FOUND, "Not Found".to_string())
    } else if let Some(e) = err.find::<Error>() {
        match e {
            Error::InvalidProejctId => (StatusCode::BAD_REQUEST, e.to_string()),
            Error::NoPermissionError => (StatusCode::UNAUTHORIZED, e.to_string()),
            Error::JWTTokenError => (StatusCode::UNAUTHORIZED, e.to_string()),
            Error::JWTTokenExpiredError => (StatusCode::UNAUTHORIZED, e.to_string()),
            Error::JWTTokenCreationError => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            _ => (StatusCode::BAD_REQUEST, e.to_string()),
        }
    } else if err.find::<warp::reject::MethodNotAllowed>().is_some() {
        (StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed".to_string())
    } else {
        if COMMAND.debug() {
            error!("{:?}", err);
        }

        (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error".to_string())
    };

    let json = warp::reply::json(&ErrorResponse {
        status: code.to_string(),
        message,
    });

    Ok(warp::reply::with_status(json, code))
}

#[derive(Debug)]
pub enum GraphQLServerError {
    QueryError(String),
    InternalError(String),
}

impl warp::reject::Reject for GraphQLServerError {}

impl fmt::Display for GraphQLServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            GraphQLServerError::QueryError(ref e) => {
                write!(f, "GraphQL server error (query error): {}", e)
            }
            GraphQLServerError::InternalError(ref e) => {
                write!(f, "GraphQL server error (internal error): {}", e)
            }
        }
    }
}

impl std::error::Error for GraphQLServerError {
    fn description(&self) -> &str {
        "Failed to process the GraphQL request"
    }

    fn cause(&self) -> Option<&dyn std::error::Error> {
        match *self {
            GraphQLServerError::QueryError(_) => None,
            GraphQLServerError::InternalError(_) => None,
        }
    }
}
