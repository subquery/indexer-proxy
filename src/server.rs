#![deny(warnings)]
use std::net::Ipv4Addr;

use serde::Serialize;
use serde_json::{json, Value};
use tracing::info;
use warp::{reject, reply, Filter, Reply};

use crate::auth::{self};
use crate::constants::HEADERS;
use crate::error::handle_rejection;
use crate::project::PROJECTS;
use crate::query::METADATA_QUERY;
use crate::request::graphql_request;
use crate::types::WebResult;
use crate::{account, prometheus};

#[derive(Serialize)]
pub struct QueryUri {
    /// the url refer to specific project
    pub uri: String,
}

#[derive(Serialize)]
pub struct QueryToken {
    /// jwt auth token
    pub token: String,
}

pub async fn start_server(host: &str, port: u16) {
    // create routes
    let token_route = warp::path!("token")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(generate_token);

    let query_route = warp::path!("query" / String)
        .and(warp::post())
        .and(warp::body::json())
        .and_then(query_handler);

    let metadata_route = warp::path!("metadata" / String)
        .and(warp::get())
        .and_then(metadata_handler);

    // chain the routes
    let routes = token_route
        .or(query_route)
        .or(metadata_route)
        .recover(handle_rejection);
    let cors = warp::cors()
        .allow_any_origin()
        .allow_headers(HEADERS)
        .allow_methods(vec!["GET", "POST"]);

    let ip_address: Ipv4Addr = host.parse().unwrap_or(Ipv4Addr::LOCALHOST);
    warp::serve(routes.with(cors)).run((ip_address, port)).await;
}

pub async fn generate_token(payload: auth::Payload) -> WebResult<impl Reply> {
    // TODO: request to coordiantor service to verify the account has valid service agreement with indexer
    let _ = match PROJECTS::get(&payload.deployment_id) {
        Ok(url) => url,
        Err(e) => return Err(reject::custom(e)),
    };

    let token = auth::create_jwt(payload).map_err(|e| reject::custom(e))?;
    Ok(reply::json(&QueryToken { token }))
}

pub async fn query_handler(id: String, query: Value) -> WebResult<impl Reply> {
    // if id != deployment_id {
    //     return Err(reject::custom(Error::JWTTokenError));
    // };

    let query_url = match PROJECTS::get(&id) {
        Ok(url) => url,
        Err(e) => return Err(reject::custom(e)),
    };

    prometheus::push_query_metrics(id.to_owned());

    let response = graphql_request(&query_url, &query).await;
    match response {
        Ok(result) => Ok(reply::json(&result)),
        Err(e) => Err(reject::custom(e)),
    }
}

pub async fn metadata_handler(id: String) -> WebResult<impl Reply> {
    let query_url = match PROJECTS::get(&id) {
        Ok(url) => url,
        Err(e) => return Err(reject::custom(e)),
    };

    // TODO: move to other place
    account::update_account_metadata();

    let query = json!({ "query": METADATA_QUERY });
    let response = graphql_request(&query_url, &query).await;
    match response {
        Ok(result) => Ok(reply::json(&result)),
        Err(e) => Err(reject::custom(e)),
    }
}
