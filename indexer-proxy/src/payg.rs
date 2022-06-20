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

//! Pay-As-You-Go with state channel helper functions.

use serde_json::{json, Value};
use subql_proxy_utils::{
    error::Error,
    payg::{convert_sign_to_string, OpenState, QueryState},
    request::graphql_request,
    types::WebResult,
};
use warp::{
    filters::header::headers_cloned,
    http::header::{HeaderMap, HeaderValue, AUTHORIZATION},
    reject, Filter, Rejection,
};
use web3::{
    signing::SecretKeyRef,
    types::{Address, U256},
};

use crate::account::ACCOUNT;
use crate::cli::COMMAND;

pub const PRICE: u64 = 10; // TODO delete

pub async fn open_state(body: &Value) -> Result<Value, Error> {
    let mut state = OpenState::from_json(body)?;

    let account = ACCOUNT.read().await;
    let key = SecretKeyRef::new(&account.controller_sk);
    state.sign(key, false)?;
    drop(account);

    let (_, _consumer) = state.recover()?;

    let url = COMMAND.service_url();

    let mdata = format!(
        r#"mutation {{
  channelOpen(id:"{:#X}", indexer:"{:?}", consumer:"{:?}", balance:{}, expiration:{}, callback:"0x{}", lastIndexerSign:"0x{}", lastConsumerSign:"0x{}") {{
    lastPrice
  }}
}}
"#,
        state.channel_id,
        state.indexer,
        state.consumer,
        state.amount,
        state.expiration,
        hex::encode(&state.callback),
        convert_sign_to_string(&state.indexer_sign),
        convert_sign_to_string(&state.consumer_sign)
    );

    let query = json!({ "query": mdata });
    let result = graphql_request(&url, &query)
        .await
        .map_err(|_| Error::ServiceException)?;
    let price = result
        .get("data")
        .ok_or(Error::ServiceException)?
        .get("channelOpen")
        .ok_or(Error::ServiceException)?
        .get("lastPrice")
        .ok_or(Error::ServiceException)?
        .as_i64()
        .ok_or(Error::ServiceException)?;
    state.next_price = U256::from(price);

    Ok(state.to_json())
}

pub fn with_state() -> impl Filter<Extract = ((QueryState, Address),), Error = Rejection> + Clone {
    headers_cloned()
        .map(move |headers: HeaderMap<HeaderValue>| (headers))
        .and_then(authorize)
}

async fn authorize(headers: HeaderMap<HeaderValue>) -> WebResult<(QueryState, Address)> {
    let header = headers
        .get(AUTHORIZATION)
        .and_then(|x| x.to_str().ok())
        .ok_or(reject::custom(Error::NoPermissionError))?;

    let mut state = match serde_json::from_str::<Value>(header) {
        Ok(v) => QueryState::from_json(&v)?,
        Err(_) => return Err(reject::custom(Error::InvalidAuthHeaderError)),
    };
    state.next_price = U256::from(PRICE);

    let account = ACCOUNT.read().await;
    let key = SecretKeyRef::new(&account.controller_sk);
    state.sign(key, false)?;
    drop(account);
    let (_, signer) = state.recover()?;

    let url = COMMAND.service_url();
    let mdata = format!(
        r#"mutation {{
  channelUpdate(id:"{:#X}", count:{}, isFinal:{}, price:{}, indexerSign:"0x{}", consumerSign:"0x{}") {{ id }}
}}
"#,
        state.channel_id,
        state.count,
        state.is_final,
        state.price,
        convert_sign_to_string(&state.indexer_sign),
        convert_sign_to_string(&state.consumer_sign)
    );

    let query = json!({ "query": mdata });
    let result = graphql_request(&url, &query)
        .await
        .map_err(|_| reject::custom(Error::ServiceException))?;

    let _ = result.get("data").ok_or(reject::custom(Error::ServiceException))?;

    Ok((state, signer))
}
