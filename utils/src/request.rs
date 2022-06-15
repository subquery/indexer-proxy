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

use once_cell::sync::Lazy;
use reqwest::{
    header::{CONNECTION, CONTENT_TYPE},
    Client,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use serde_with::skip_serializing_none;

use crate::{
    constants::{APPLICATION_JSON, KEEP_ALIVE, AUTHORIZATION},
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

// Request to graphql service.
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
        Err(e) => return Err(GraphQLServerError::InternalError(format!("Parse result error:{}", e))),
    };

    Ok(json_data)
}

// Request to indexer proxy
pub async fn proxy_request(
    method: &str,
    url: &str,
    path: &str,
    token: &str,
    query: String,
    headers: Vec<(String, String)>,
) -> Result<Value, Value> {
    let url = format!("{}/{}", url, path);
    let token = format!("Bearer {}", token);

    let res = match method.to_lowercase().as_str() {
        "get" => {
            let mut req = REQUEST_CLIENT.get(url);
            for (k, v) in headers {
                req = req.header(k, v);
            }
            req.header(AUTHORIZATION, token).send().await
        }
        _ => {
            let mut req = REQUEST_CLIENT.post(url).header("content-type", "application/json");
            for (k, v) in headers {
                req = req.header(k, v);
            }
            req.header(AUTHORIZATION, token).body(query).send().await
        }
    };

    match res {
        Ok(res) => match res.error_for_status() {
            Ok(res) => match res.text().await {
                Ok(data) => match serde_json::from_str(&data) {
                    Ok(data) => Ok(data),
                    Err(_err) => Ok(json!(data)),
                },
                Err(err) => Err(json!(err.to_string())),
            },
            Err(err) => Err(json!(err.to_string())),
        },
        Err(err) => Err(json!(err.to_string())),
    }
}

// Request to jsonrpc service.(P2P RPC)
pub async fn jsonrpc_request(id: u64, url: &str, method: &str, params: Vec<Value>) -> Result<Value, Value> {
    let res = REQUEST_CLIENT
        .post(url)
        .header("content-type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))
        .send()
        .await
        .unwrap();

    match res.error_for_status() {
        Ok(res) => match res.json::<Value>().await {
            Ok(data) => {
                if data.get("result").is_some() {
                    if data["result"].is_array() {
                        let mut res = vec![];
                        for i in data["result"].as_array().unwrap() {
                            let i_str = i.as_str().unwrap();
                            match serde_json::from_str::<Value>(i_str) {
                                Ok(r) => res.push(r),
                                Err(_) => res.push(Value::from(i_str)),
                            }
                        }
                        Ok(json!(res))
                    } else {
                        let res = data["result"].as_str().unwrap_or("");
                        if let Ok(json) = serde_json::from_str::<Value>(res) {
                            if json.get("errors").is_some() {
                                Err(json)
                            } else {
                                Ok(json)
                            }
                        } else {
                            Ok(json!(res))
                        }
                    }
                } else {
                    if data.get("error").is_some() {
                        Err(json!(data["error"]["message"]))
                    } else {
                        Ok(json!("ok"))
                    }
                }
            }
            Err(err) => Err(json!(err.to_string())),
        },
        Err(err) => Err(json!(err.to_string())),
    }
}
