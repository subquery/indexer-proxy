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

use async_trait::async_trait;
use serde_json::{json, Value};
use subql_proxy_utils::{
    p2p::{P2pHandler, Request, Response},
    payg::QueryState,
    request::graphql_request,
};
use web3::{signing::SecretKeyRef, types::U256};

use crate::account::ACCOUNT;
use crate::payg::{open_state, PRICE};
use crate::project::get_project;

pub struct IndexerP2p;

#[async_trait]
impl P2pHandler for IndexerP2p {
    async fn request(request: Request) -> Response {
        // handle request
        match request {
            Request::Query(project, query, sign) => {
                let res_data = query_request(project, query).await;
                let res_sign = if sign.len() > 0 {
                    handle(&sign).await
                } else {
                    Response::Sign("".to_owned())
                };
                res_data.with_sign(res_sign)
            }
            Request::StateChannel(infos) => handle(&infos).await,
        }
    }

    async fn event() {
        todo!()
    }
}

/// Handle the state channel request/response infos.
async fn handle(infos: &str) -> Response {
    let params = serde_json::from_str::<Value>(infos).unwrap_or(Value::default());
    if params.get("method").is_none() || params.get("state").is_none() {
        return Response::Error("Invalid request".to_owned());
    }
    match params["method"].as_str().unwrap() {
        "info" => {
            let account = ACCOUNT.read().await;
            let data = json!({
                "indexer": format!("{:?}", account.indexer),
                "controller": format!("{:?}", account.controller),
                "price": U256::from(PRICE),
            });
            drop(account);
            Response::Sign(serde_json::to_string(&data).unwrap())
        }
        "open" => match open_state(&params).await {
            Ok(state) => Response::Sign(serde_json::to_string(&state["state"]).unwrap()),
            Err(err) => Response::Error(err.to_string()),
        },
        "query" => match QueryState::from_json(&params["state"]) {
            Ok(mut state) => {
                state.next_price = U256::from(PRICE);
                let account = ACCOUNT.read().await;
                let key = SecretKeyRef::new(&account.controller_sk);
                match state.sign(key, false) {
                    Err(err) => return Response::Error(err.to_string()),
                    _ => {}
                }
                let _signer = match state.recover() {
                    Ok((_, consumer)) => consumer,
                    Err(err) => return Response::Error(err.to_string()),
                };
                drop(account);

                // TODO query state to coordiantor

                Response::Sign(serde_json::to_string(&state.to_json()).unwrap())
            }
            Err(err) => Response::Error(err.to_string()),
        },
        _ => Response::Error("Invalid request".to_owned()),
    }
}

async fn query_request(project: String, query: String) -> Response {
    match (get_project(&project), serde_json::from_str(&query)) {
        (Ok(url), Ok(query)) => match graphql_request(&url, &query).await {
            Ok(value) => match value.pointer("/data") {
                Some(data) => Response::RawData(serde_json::to_string(data).unwrap()),
                _ => Response::Error("Data is missing".to_owned()),
            },
            Err(err) => Response::Error(err.to_string()),
        },
        _ => Response::Error("Project is missing".to_owned()),
    }
}
