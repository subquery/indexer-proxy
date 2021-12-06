#![deny(warnings)]
use serde::Serialize;
use tracing_subscriber::fmt::format::FmtSpan;
use warp::{reject, reply, Filter, Reply};

use crate::auth;
use crate::auth::with_auth;
use crate::auth::User;
use crate::error;
use crate::project::PROJECTS;
use crate::request::graphql_request;
use crate::request::QueryBody;
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

pub async fn start_server(port: u16) {
    // configure the tracing subscriber
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "tracing=info,warp=debug".to_owned());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    // create routes
    let token_query = warp::query::<User>()
        .map(Some)
        .or_else(|_| async { Ok::<(Option<User>,), std::convert::Infallible>((None,)) });

    let token_route = warp::path!("token")
        .and(warp::get())
        .and(token_query)
        .and_then(get_token);

    let discovery_route = warp::path!("discovery" / String)
        .and(warp::get())
        .and_then(discovery_handler);

    let query_route = warp::path!("query" / String)
        .and(warp::post())
        .and(with_auth())
        .and(warp::body::json())
        .and_then(query_handler);

    // chain the routes
    let routes = discovery_route
        .or(token_route)
        .or(query_route)
        .recover(error::handle_rejection);

    warp::serve(routes).run(([127, 0, 0, 1], port)).await;
}

pub async fn discovery_handler(deployment_id: String) -> WebResult<impl Reply> {
    // TODO: convert deployment_id to a hash value, return `/query/hash_value` endpoint
    match PROJECTS::get(&deployment_id) {
        Ok(_) => Ok(reply::json(&QueryUri {
            uri: format!("/query/{}", deployment_id),
        })),
        _ => Err(reject::custom(error::Error::InvalidProejctId)),
    }
}

pub async fn get_token(request_praram: Option<User>) -> WebResult<impl Reply> {
    let user = match request_praram {
        Some(user) => user,
        None => return Err(reject::custom(error::Error::InvalidQueryParamsError)),
    };

    let _ = match PROJECTS::get(&user.deployment_id) {
        Ok(url) => url,
        Err(e) => return Err(reject::custom(e)),
    };

    let token = auth::create_jwt(user).map_err(|e| reject::custom(e))?;
    return Ok(reply::json(&QueryToken { token }));
}

pub async fn query_handler(
    deployment_id: String,
    _: String,
    body: QueryBody,
) -> WebResult<impl Reply> {
    let query_url = match PROJECTS::get(&deployment_id) {
        Ok(url) => url,
        Err(e) => return Err(reject::custom(e)),
    };

    let response = graphql_request(&query_url, body.query).await;
    match response {
        Ok(result) => Ok(reply::json(&result)),
        Err(e) => Err(reject::custom(e)),
    }
}
