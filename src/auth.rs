use crate::{cli, eip712::recover_signer, error::Error, types::Result};
use chrono::prelude::*;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tracing::debug;
use warp::{
    filters::header::headers_cloned,
    http::header::{HeaderMap, HeaderValue, AUTHORIZATION},
    reject, Filter, Rejection,
};

use crate::types::WebResult;

const BEARER: &str = "Bearer ";
// FIXME: use `secret_key` from commandline args
const JWT_SECRET: &[u8] = b"secret";

#[derive(Serialize, Deserialize, Debug)]
pub struct Payload {
    /// indexer address
    pub indexer: String,
    /// indexer address
    pub consumer: Option<String>,
    /// service agreement contract address
    pub agreement: Option<String>,
    /// deployment id for the proejct
    pub deployment_id: String,
    /// signature of user
    pub signature: String,
    /// timestamp
    pub timestamp: i64,
    /// chain id
    pub chain_id: i64,
}

#[derive(Serialize, Deserialize)]
struct Claims {
    /// ethereum address
    pub indexer: String,
    /// deployment id for the proejct
    pub deployment_id: String,
    /// issue timestamp
    pub iat: i64,
    /// token expiration
    exp: i64,
}

type RequestHeader = HeaderMap<HeaderValue>;

pub fn create_jwt(payload: Payload) -> Result<String> {
    let expiration = Utc::now()
        .checked_add_signed(chrono::Duration::hours(
            cli::CommandLineArgs::token_duration(),
        ))
        .expect("valid timestamp")
        .timestamp_millis();

    let msg_verified = true; // verify_message(&payload).map_err(|_| Error::JWTTokenCreationError)?;
    if !msg_verified || (Utc::now().timestamp_millis() - payload.timestamp).abs() > 120000 {
        return Err(Error::JWTTokenCreationError);
    }

    let header = Header::new(Algorithm::HS512);
    let claims = Claims {
        indexer: payload.indexer,
        deployment_id: payload.deployment_id,
        iat: payload.timestamp,
        exp: expiration,
    };

    encode(&header, &claims, &EncodingKey::from_secret(JWT_SECRET))
        .map_err(|_| Error::JWTTokenCreationError)
}

pub fn with_auth() -> impl Filter<Extract = (String,), Error = Rejection> + Clone {
    headers_cloned()
        .map(move |headers: RequestHeader| (headers))
        .and_then(authorize)
}

async fn authorize(headers: RequestHeader) -> WebResult<String> {
    if !cli::CommandLineArgs::auth() {
        return Ok(String::from(""));
    }

    match jwt_from_header(&headers) {
        Ok(jwt) => {
            let decoded = decode::<Claims>(
                &jwt,
                &DecodingKey::from_secret(JWT_SECRET),
                &Validation::new(Algorithm::HS512),
            )
            .map_err(|_| reject::custom(Error::JWTTokenError))?;

            if decoded.claims.exp < Utc::now().timestamp_millis() {
                return Err(reject::custom(Error::JWTTokenExpiredError));
            }

            Ok(decoded.claims.deployment_id)
        }
        Err(e) => return Err(reject::custom(e)),
    }
}

fn jwt_from_header(headers: &HeaderMap<HeaderValue>) -> Result<String> {
    let header = match headers.get(AUTHORIZATION) {
        Some(v) => v,
        None => return Err(Error::NoPermissionError),
    };
    let auth_header = match std::str::from_utf8(header.as_bytes()) {
        Ok(v) => v,
        Err(_) => return Err(Error::NoPermissionError),
    };
    if !auth_header.starts_with(BEARER) {
        return Err(Error::InvalidAuthHeaderError);
    }

    Ok(auth_header.trim_start_matches(BEARER).to_owned())
}

fn verify_message(payload: &Payload) -> Result<bool> {
    let message = format!(
        "{}{}{}",
        payload.indexer, payload.deployment_id, payload.timestamp
    );
    let signer = recover_signer(message, &payload.signature).unwrap();

    debug!("compare pubkey: {}", signer);

    // TODO: verify message basing on the payload
    // 1. if signer is indexer itself, return the token
    // 2. if singer is consumer, check whether the agreement is expired and the it is consistent with `indexer` and `consumer`

    Ok(signer == payload.indexer.as_str().to_lowercase())
}
