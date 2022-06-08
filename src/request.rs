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
        Err(e) => return Err(GraphQLServerError::InternalError(format!("Parse result error:{}", e))),
    };

    Ok(json_data)
}
