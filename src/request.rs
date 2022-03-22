use lazy_static::lazy_static;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::skip_serializing_none;

use std::error::Error;
use std::fmt;

use crate::constants::{APPLICATION_JSON, CONTENT_TYPE};

// FIXME: integrate these errors to `error` module
/// Errors that can occur while processing incoming requests.
#[derive(Debug)]
pub enum GraphQLServerError {
    ClientError(String),
    QueryError(String),
    InternalError(String),
}

impl warp::reject::Reject for GraphQLServerError {}

impl fmt::Display for GraphQLServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            GraphQLServerError::ClientError(ref e) => {
                write!(f, "GraphQL server error (client error): {}", e)
            }
            GraphQLServerError::QueryError(ref e) => {
                write!(f, "GraphQL server error (query error): {}", e)
            }
            GraphQLServerError::InternalError(ref e) => {
                write!(f, "GraphQL server error (internal error): {}", e)
            }
        }
    }
}

impl Error for GraphQLServerError {
    fn description(&self) -> &str {
        "Failed to process the GraphQL request"
    }

    fn cause(&self) -> Option<&dyn Error> {
        match *self {
            GraphQLServerError::ClientError(_) => None,
            GraphQLServerError::QueryError(_) => None,
            GraphQLServerError::InternalError(_) => None,
        }
    }
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub struct GraphQLQuery {
    /// The GraphQL query, as a string.
    pub query: String,
    ///  The GraphQL query variables
    pub variables: Option<Value>,
    /// The GraphQL operation name, as a string.
    #[serde(rename = "operationName")]
    pub operation_name: Option<String>,
}

impl GraphQLQuery {
    pub fn new(query: String) -> Self {
        Self {
            query,
            operation_name: None,
            variables: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct QueryBody {
    pub query: GraphQLQuery,
}

lazy_static! {
    pub static ref REQUEST_CLIENT: Client = reqwest::Client::new();
}

// TODO: reorganise the errors
pub async fn graphql_request(uri: &str, query: &str) -> Result<Value, GraphQLServerError> {
    // let body = serde_json::to_string(&query)
    //     .map_err(|e| GraphQLServerError::ClientError(format!("Invalid query body: {}", e)))?;
    let body = query.to_string();

    let response_result = REQUEST_CLIENT
        .post(uri)
        .header(CONTENT_TYPE, APPLICATION_JSON)
        .body(body)
        .send()
        .await;

    let res = match response_result {
        Ok(res) => res,
        Err(e) => return Err(GraphQLServerError::QueryError(format!("{}", e))),
    };

    let json_result = res.json().await;
    let json_data: Value = match json_result {
        Ok(res) => res,
        Err(e) => {
            return Err(GraphQLServerError::InternalError(format!(
                "Parse result error:{}",
                e
            )))
        }
    };

    Ok(json_data)
}
