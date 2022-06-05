#![deny(warnings)]
use std::net::Ipv4Addr;

use serde::Serialize;
use serde_json::{json, Value};
use warp::{reject, reply, Filter, Reply};
use web3::types::Address;

use crate::auth::{self, with_auth};
use crate::constants::HEADERS;
use crate::error::{handle_rejection, Error};
use crate::payg::{open_state, with_state, QueryState};
use crate::project::get_project;
use crate::query::METADATA_QUERY;
use crate::request::graphql_request;
use crate::types::WebResult;
use crate::{account, cli::COMMAND, prometheus};

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
    // create token for query.
    let token_route = warp::path!("token")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(generate_token);

    // query with agreement.
    let query_route = warp::path!("query" / String)
        .and(warp::post())
        .and(with_auth())
        .and(warp::body::json())
        .and_then(query_handler);

    // open a state channel for payg.
    let open_route = warp::path!("open")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(generate_payg);

    // query with Pay-As-You-Go with state channel
    let payg_route = warp::path!("payg" / String)
        .and(warp::post())
        .and(with_state())
        .and(warp::body::json())
        .and_then(payg_handler);

    // query the metadata (indexer, controller, payg-price)
    let metadata_route = warp::path!("metadata" / String)
        .and(warp::get())
        .and_then(metadata_handler);

    // chain the routes
    let routes = token_route
        .or(query_route)
        .or(open_route)
        .or(payg_route)
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
    let _ = match get_project(&payload.deployment_id) {
        Ok(url) => url,
        Err(e) => return Err(reject::custom(e)),
    };

    let token = auth::create_jwt(payload).map_err(|e| reject::custom(e))?;
    Ok(reply::json(&QueryToken { token }))
}

pub async fn query_handler(
    id: String,
    deployment_id: String,
    query: Value,
) -> WebResult<impl Reply> {
    if COMMAND.auth() && id != deployment_id {
        return Err(reject::custom(Error::JWTTokenError));
    };

    let query_url = match get_project(&id) {
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

pub async fn generate_payg(payload: Value) -> WebResult<impl Reply> {
    let state = open_state(&payload).await.map_err(|e| reject::custom(e))?;
    Ok(reply::json(&state))
}

pub async fn payg_handler(
    id: String,
    state: (QueryState, Address),
    query: Value,
) -> WebResult<impl Reply> {
    let query_url = match get_project(&id) {
        Ok(url) => url,
        Err(e) => return Err(reject::custom(e)),
    };
    prometheus::push_query_metrics(id);

    match graphql_request(&query_url, &query).await {
        Ok(result) => {
            let string = serde_json::to_string(&result).unwrap(); // safe unwrap
            let _sign = account::sign_message(&string.as_bytes()); // TODO add to header

            // TODO add state to header and request to coordiantor know the response.
            let (_state, _signer) = state;

            Ok(reply::json(&result))
        }
        Err(e) => Err(reject::custom(e)),
    }
}

pub async fn metadata_handler(id: String) -> WebResult<impl Reply> {
    let query_url = match get_project(&id) {
        Ok(url) => url,
        Err(e) => return Err(reject::custom(e)),
    };

    // TODO: move to other place
    let _ = account::fetch_account_metadata().await;

    let query = json!({ "query": METADATA_QUERY });
    let response = graphql_request(&query_url, &query).await;
    match response {
        Ok(result) => Ok(reply::json(&result)),
        Err(e) => Err(reject::custom(e)),
    }
}
