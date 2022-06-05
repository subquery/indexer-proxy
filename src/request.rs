use once_cell::sync::Lazy;
use reqwest::{
    header::{CONNECTION, CONTENT_TYPE},
    Client,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::skip_serializing_none;

use crate::{
    constants::{APPLICATION_JSON, KEEP_ALIVE},
    error::GraphQLServerError,
};

pub static REQUEST_CLIENT: Lazy<Client> = Lazy::new(|| reqwest::Client::new());

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

pub async fn graphql_request(uri: &str, query: &Value) -> Result<Value, GraphQLServerError> {
    let response_result = REQUEST_CLIENT
        .post(uri)
        .header(CONTENT_TYPE, APPLICATION_JSON)
        .header(CONNECTION, KEEP_ALIVE)
        .body(query.to_string())
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
