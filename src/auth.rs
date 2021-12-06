use crate::{error::Error, types::Result};
use chrono::prelude::*;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use warp::{
    filters::header::headers_cloned,
    http::header::{HeaderMap, HeaderValue, AUTHORIZATION},
    reject, Filter, Rejection,
};

use crate::types::WebResult;

const BEARER: &str = "Bearer ";
const JWT_SECRET: &[u8] = b"secret";

#[derive(Serialize, Deserialize)]
pub struct User {
    /// ethereum address
    pub user_id: String,
    /// deployment id for the proejct
    pub deployment_id: String,
}

#[derive(Serialize, Deserialize)]
struct Claims {
    /// query service user
    user: User,
    /// token expiration
    exp: usize,
}

type RequestHeader = HeaderMap<HeaderValue>;

pub fn create_jwt(user: User) -> Result<String> {
    let expiration = Utc::now()
        .checked_add_signed(chrono::Duration::days(1))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        user,
        exp: expiration as usize,
    };
    let header = Header::new(Algorithm::HS512);

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

            Ok(decoded.claims.exp.to_string())
        }
        Err(e) => return Err(reject::custom(e)),
    }
}

fn jwt_from_header(headers: &HeaderMap<HeaderValue>) -> Result<String> {
    let header = match headers.get(AUTHORIZATION) {
        Some(v) => v,
        None => return Err(Error::NoAuthHeaderError),
    };
    let auth_header = match std::str::from_utf8(header.as_bytes()) {
        Ok(v) => v,
        Err(_) => return Err(Error::NoAuthHeaderError),
    };
    if !auth_header.starts_with(BEARER) {
        return Err(Error::InvalidAuthHeaderError);
    }

    Ok(auth_header.trim_start_matches(BEARER).to_owned())
}
