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

use serde_json::Value;
use std::net::Ipv4Addr;
use subql_proxy_utils::{
    constants::HEADERS,
    error::{handle_rejection, Error},
    payg::{convert_recovery_sign, convert_sign_to_bytes, convert_string_to_sign, OpenState, QueryState},
    types::WebResult,
};
use warp::{reject, reply, Filter, Reply};
use web3::{
    contract::tokens::Tokenizable,
    ethabi::encode,
    signing::{keccak256, recover},
    types::{Address, U256},
};

use crate::cli::COMMAND;
use crate::payg::{add_project, StateChannel};

pub async fn start_server(host: &str, port: u16) {
    // query with agreement.
    let query_route = warp::path!("query" / String)
        .and(warp::post())
        .and(warp::body::json())
        .and_then(query_handler);

    // open a state channel for payg.
    let open_route = warp::path!("open")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(open_payg);

    // graphql playground page.
    let pg_route = warp::path!("graphql").map(|| reply::html(include_str!("./playground.html")));

    // chain the routes
    let routes = query_route
        .or(open_route)
        .or(pg_route)
        .recover(|err| handle_rejection(err, COMMAND.dev()));
    let cors = warp::cors()
        .allow_any_origin()
        .allow_headers(HEADERS)
        .allow_methods(vec!["GET", "POST"]);

    let ip_address: Ipv4Addr = host.parse().unwrap_or(Ipv4Addr::LOCALHOST);
    warp::serve(routes.with(cors)).run((ip_address, port)).await;
}

pub async fn query_handler(id: String, query: Value) -> WebResult<impl Reply> {
    let channel = StateChannel::get(&id).await?;
    let state = channel.next_query(COMMAND.signer())?;

    let raw_state = serde_json::to_string(&state.to_json()).unwrap();
    let raw_query = serde_json::to_string(&query).unwrap();
    let res = COMMAND.indexer.query(id, raw_query, raw_state).await;

    match res {
        Ok(fulldata) => {
            let (query, raw_data) = (&fulldata[0], &fulldata[1]);

            // TODO save state to db.
            let _state = QueryState::from_json(&raw_data).unwrap();

            Ok(reply::json(&query))
        }
        Err(err) => {
            info!("Open Error: {}", err);
            Err(reject::custom(Error::ServiceException))
        }
    }
}

pub async fn open_payg(payload: Value) -> WebResult<impl Reply> {
    let channel_id: U256 = payload
        .get("channelId")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse().ok())
        .ok_or(reject::custom(Error::InvalidRequest))?;
    let indexer = payload
        .get("indexer")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse().ok())
        .ok_or(reject::custom(Error::InvalidRequest))?;
    let amount: U256 = payload
        .get("amount")
        .and_then(|v| v.as_str())
        .and_then(|v| U256::from_dec_str(v).ok())
        .ok_or(reject::custom(Error::InvalidRequest))?;
    let expiration: U256 = payload
        .get("expiration")
        .and_then(|v| v.as_str())
        .and_then(|v| U256::from_dec_str(v).ok())
        .ok_or(reject::custom(Error::InvalidRequest))?;
    let consumer: Address = payload
        .get("consumer")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse().ok())
        .ok_or(reject::custom(Error::InvalidRequest))?;
    let callback = payload
        .get("sign")
        .and_then(|v| v.as_str())
        .ok_or(reject::custom(Error::InvalidRequest))?;
    let sign = convert_string_to_sign(callback);

    // check the sign.
    let msg = encode(&[channel_id.into_token(), amount.into_token()]);
    let mut bytes = "\x19Ethereum Signed Message:\n32".as_bytes().to_vec();
    bytes.extend(keccak256(&msg));
    let payload = keccak256(&bytes);
    let (i_sign, i_id) = convert_recovery_sign(&sign);
    let signer = recover(&payload, &i_sign, i_id).map_err(|_| Error::InvalidSignature)?;
    if signer != consumer {
        return Err(reject::custom(Error::InvalidSignature));
    }

    // TODO handle consumer
    let state = OpenState::consumer_generate(
        Some(channel_id),
        indexer,
        COMMAND.contract(),
        amount,
        expiration,
        convert_sign_to_bytes(&sign),
        COMMAND.signer(),
    )?;
    let raw_state = serde_json::to_string(&state.to_json()).unwrap();
    let res = COMMAND.indexer.open(raw_state).await;

    match res {
        Ok(data) => {
            let state = OpenState::from_json(&data).unwrap();
            let channel = state.channel_id;
            let projects: Vec<String> = data
                .get("projects")
                .and_then(|v| v.as_array())
                .unwrap_or(&vec![])
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_owned())
                .collect();
            StateChannel::add(state).await;
            for project in projects {
                add_project(project, channel).await;
            }
            Ok(reply::json(&data))
        }
        Err(err) => {
            info!("Open Error: {}", err);
            Err(reject::custom(Error::ServiceException))
        }
    }
}
