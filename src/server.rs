#![deny(warnings)]
use std::net::Ipv4Addr;

use serde::Serialize;
use warp::{reject, reply, Filter, Reply};

use crate::auth;
use crate::auth::User;
use crate::constants::METADATA_QUERY;
use crate::error;
use crate::project::PROJECTS;
use crate::request::graphql_request;
use crate::traits::Hash;
use crate::types::WebResult;

// TODO: refactor to separate `mod`
// mod `handlers` | mod `filters` -> routes |

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
    let token_query = warp::query::<User>()
        .map(Some)
        .or_else(|_| async { Ok::<(Option<User>,), std::convert::Infallible>((None,)) });

    let token_route = warp::path!("token")
        .and(warp::get())
        .and(token_query)
        .and_then(get_token);

    let query_route = warp::path!("query" / String)
        .and(warp::post())
        // .and(with_auth()) // temporary disabled the auth check
        .and(warp::body::json())
        .and_then(query_handler);

    let metadata_route = warp::path!("metadata" / String)
        .and(warp::get())
        .and_then(metadata_handler);

    // chain the routes
    let routes = token_route
        .or(query_route)
        .or(metadata_route)
        .recover(error::handle_rejection);
    let cors = warp::cors()
        .allow_any_origin()
        .allow_header("content-type")
        .allow_methods(vec!["GET", "POST"]);

    let ip_address: Ipv4Addr = host.parse().unwrap_or(Ipv4Addr::LOCALHOST);
    warp::serve(routes.with(cors)).run((ip_address, port)).await;
}

pub async fn get_token(request_praram: Option<User>) -> WebResult<impl Reply> {
    let user = match request_praram {
        Some(user) => user,
        None => return Err(reject::custom(error::Error::InvalidQueryParamsError)),
    };

    let _ = match PROJECTS::get(&user.deployment_id.hash()) {
        Ok(url) => url,
        Err(e) => return Err(reject::custom(e)),
    };

    let token = auth::create_jwt(user).map_err(|e| reject::custom(e))?;
    Ok(reply::json(&QueryToken { token }))
}

pub async fn query_handler(id: String, query: String) -> WebResult<impl Reply> {
    let query_url = match PROJECTS::get(&id.hash()) {
        Ok(url) => url,
        Err(e) => return Err(reject::custom(e)),
    };

    let response = graphql_request(&query_url, &query).await;
    match response {
        Ok(result) => Ok(reply::json(&result)),
        Err(e) => Err(reject::custom(e)),
    }
}

pub async fn metadata_handler(id: String) -> WebResult<impl Reply> {
    let query_url = match PROJECTS::get(&id.hash()) {
        Ok(url) => url,
        Err(e) => return Err(reject::custom(e)),
    };

    let response = graphql_request(&query_url, METADATA_QUERY).await;
    match response {
        Ok(result) => Ok(reply::json(&result)),
        Err(e) => Err(reject::custom(e)),
    }
}
