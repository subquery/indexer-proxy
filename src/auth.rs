use crate::{error::Error, types::Result};
use chrono::prelude::*;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use warp::{
    filters::header::headers_cloned,
    http::header::{HeaderMap, HeaderValue, AUTHORIZATION},
    reject, Filter, Rejection,
};
use web3::signing::{keccak256, recover};

use crate::types::WebResult;

const BEARER: &str = "Bearer ";
// FIXME: use `secret_key` from commandline args
const JWT_SECRET: &[u8] = b"secret";

#[derive(Serialize, Deserialize, Debug)]
pub struct Payload {
    /// ethereum address
    pub user_id: String,
    /// deployment id for the proejct
    pub deployment_id: String,
    /// signature of user
    pub signature: String,
    /// timestamp
    pub timestamp: i64,
}

#[derive(Serialize, Deserialize)]
struct Claims {
    /// ethereum address
    pub user_id: String,
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
        .checked_add_signed(chrono::Duration::hours(12))
        .expect("valid timestamp")
        .timestamp_millis();

    let msg_verified = verify_message(&payload).map_err(|_| Error::JWTTokenCreationError)?;
    if !msg_verified || (Utc::now().timestamp_millis() - payload.timestamp).abs() > 120000 {
        return Err(Error::JWTTokenCreationError);
    }

    let header = Header::new(Algorithm::HS512);
    let claims = Claims {
        user_id: payload.user_id,
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

fn eth_message(message: String) -> [u8; 32] {
    keccak256(
        format!(
            "{}{}{}",
            "\x19Ethereum Signed Message:\n",
            message.len(),
            message
        )
        .as_bytes(),
    )
}

fn verify_message(payload: &Payload) -> Result<bool> {
    let message = format!(
        "{}{}{}",
        payload.user_id, payload.deployment_id, payload.timestamp
    );
    let msg = eth_message(message);
    let sig = hex::decode(&payload.signature).unwrap();
    let pubkey = recover(&msg, &sig[..64], 1280).unwrap();
    let address = format!("{:02X?}", pubkey);

    Ok(address == payload.user_id.as_str().to_lowercase())
}
