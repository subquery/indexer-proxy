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

use rand_chacha::{
    rand_core::{RngCore, SeedableRng},
    ChaChaRng,
};
use serde_json::{json, Value};
use warp::{
    filters::header::headers_cloned,
    http::header::{HeaderMap, HeaderValue, AUTHORIZATION},
    reject, Filter, Rejection,
};
use web3::{
    contract::tokens::Tokenizable,
    ethabi::encode,
    signing::{keccak256, recover, Key, SecretKeyRef, Signature},
    types::{Address, H256, U256},
};

use crate::account::ACCOUNT;
use crate::cli::COMMAND;
use crate::error::Error;
use crate::request::graphql_request;
use crate::types::WebResult;

const BEARER: &str = "State ";
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
  channelOpen(id:"{:#X}", indexer:"{:?}", consumer:"{:?}", balance:{}, expiration:{}, lastIndexerSign:"0x{}", lastConsumerSign:"0x{}") {{
    lastPrice
  }}
}}
"#,
        state.channel_id,
        state.indexer,
        state.consumer,
        state.amount,
        state.expiration,
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

    println!("------------------------- 4: {}", result);
    let _ = result.get("data").ok_or(reject::custom(Error::ServiceException))?;

    Ok((state, signer))
}

pub struct OpenState {
    pub channel_id: U256,
    pub indexer: Address,
    pub consumer: Address,
    pub amount: U256,
    pub expiration: U256,
    pub indexer_sign: Signature,
    pub consumer_sign: Signature,
    pub next_price: U256,
}

impl OpenState {
    pub fn consumer_generate(
        indexer: Address,
        consumer: Address,
        amount: U256,
        expiration: U256,
        key: SecretKeyRef,
    ) -> Result<Self, Error> {
        let mut rng = ChaChaRng::from_entropy();
        let mut id = [0u64; 4]; // u256
        for i in 0..4 {
            id[i] = rng.next_u64();
        }
        let channel_id = U256(id);
        let mut state = Self {
            channel_id,
            indexer,
            consumer,
            amount,
            expiration,
            consumer_sign: default_sign(),
            indexer_sign: default_sign(),
            next_price: U256::from(0u64),
        };
        state.sign(key, true)?;
        Ok(state)
    }

    pub fn recover(&self) -> Result<(Address, Address), Error> {
        let msg = encode(&[
            self.channel_id.into_token(),
            self.indexer.into_token(),
            self.consumer.into_token(),
            self.amount.into_token(),
            self.expiration.into_token(),
        ]);
        let mut bytes = "\x19Ethereum Signed Message:\n32".as_bytes().to_vec();
        bytes.extend(keccak256(&msg));
        let payload = keccak256(&bytes);
        let (i_sign, i_id) = convert_recovery_sign(&self.indexer_sign);
        let (c_sign, c_id) = convert_recovery_sign(&self.consumer_sign);
        let indexer = recover(&payload, &i_sign, i_id).map_err(|_| Error::InvalidSignature)?;
        let consumer = recover(&payload, &c_sign, c_id).map_err(|_| Error::InvalidSignature)?;
        Ok((indexer, consumer))
    }

    pub fn sign(&mut self, key: SecretKeyRef, is_consumer: bool) -> Result<(), Error> {
        let msg = encode(&[
            self.channel_id.into_token(),
            self.indexer.into_token(),
            self.consumer.into_token(),
            self.amount.into_token(),
            self.expiration.into_token(),
        ]);
        let mut bytes = "\x19Ethereum Signed Message:\n32".as_bytes().to_vec();
        bytes.extend(keccak256(&msg));
        let payload = keccak256(&bytes);
        let sign = key.sign_message(&payload).map_err(|_| Error::InvalidSignature)?;
        if is_consumer {
            self.consumer_sign = sign;
        } else {
            self.indexer_sign = sign;
        }
        Ok(())
    }

    pub fn from_json(params: &Value) -> Result<Self, Error> {
        let channel_id: U256 = params["channelId"]
            .as_str()
            .ok_or(Error::InvalidSerialize)?
            .parse()
            .map_err(|_e| Error::InvalidSerialize)?;
        let indexer: Address = params["indexer"]
            .as_str()
            .ok_or(Error::InvalidSerialize)?
            .parse()
            .map_err(|_e| Error::InvalidSerialize)?;
        let consumer: Address = params["consumer"]
            .as_str()
            .ok_or(Error::InvalidSerialize)?
            .parse()
            .map_err(|_e| Error::InvalidSerialize)?;
        let amount = U256::from_dec_str(params["amount"].as_str().ok_or(Error::InvalidSerialize)?)
            .map_err(|_e| Error::InvalidSerialize)?;
        let expiration = U256::from_dec_str(params["expiration"].as_str().ok_or(Error::InvalidSerialize)?)
            .map_err(|_e| Error::InvalidSerialize)?;
        let indexer_sign: Signature =
            convert_string_to_sign(params["indexerSign"].as_str().ok_or(Error::InvalidSerialize)?);
        let consumer_sign: Signature =
            convert_string_to_sign(params["consumerSign"].as_str().ok_or(Error::InvalidSerialize)?);
        let next_price = U256::from_dec_str(params["nextPrice"].as_str().ok_or(Error::InvalidSerialize)?)
            .map_err(|_e| Error::InvalidSerialize)?;
        Ok(Self {
            channel_id,
            indexer,
            consumer,
            amount,
            expiration,
            indexer_sign,
            consumer_sign,
            next_price,
        })
    }

    pub fn to_json(&self) -> Value {
        json!({
            "channelId": format!("{:#X}", self.channel_id),
            "indexer": format!("{:?}", self.indexer),
            "consumer": format!("{:?}", self.consumer),
            "amount": self.amount.to_string(),
            "expiration": self.expiration.to_string(),
            "indexerSign": convert_sign_to_string(&self.indexer_sign),
            "consumerSign": convert_sign_to_string(&self.consumer_sign),
            "nextPrice": self.next_price.to_string(),
        })
    }
}

pub struct QueryState {
    pub channel_id: U256,
    pub indexer: Address,
    pub consumer: Address,
    pub count: U256,
    pub price: U256,
    pub is_final: bool,
    pub indexer_sign: Signature,
    pub consumer_sign: Signature,
    pub next_price: U256,
}

impl QueryState {
    pub fn consumer_generate(
        channel_id: U256,
        indexer: Address,
        consumer: Address,
        count: U256,
        price: U256,
        is_final: bool,
        key: SecretKeyRef,
    ) -> Result<Self, Error> {
        let mut state = Self {
            channel_id,
            indexer,
            consumer,
            count,
            price,
            is_final,
            consumer_sign: default_sign(),
            indexer_sign: default_sign(),
            next_price: U256::from(0u64),
        };
        state.sign(key, true)?;
        Ok(state)
    }

    pub fn recover(&self) -> Result<(Address, Address), Error> {
        let msg = encode(&[
            self.channel_id.into_token(),
            self.count.into_token(),
            self.price.into_token(),
            self.is_final.into_token(),
        ]);
        let mut bytes = "\x19Ethereum Signed Message:\n32".as_bytes().to_vec();
        bytes.extend(keccak256(&msg));
        let payload = keccak256(&bytes);
        let (i_sign, i_id) = convert_recovery_sign(&self.indexer_sign);
        let (c_sign, c_id) = convert_recovery_sign(&self.consumer_sign);
        let indexer = recover(&payload, &i_sign, i_id).map_err(|_| Error::InvalidSignature)?;
        let consumer = recover(&payload, &c_sign, c_id).map_err(|_| Error::InvalidSignature)?;
        Ok((indexer, consumer))
    }

    pub fn sign(&mut self, key: SecretKeyRef, is_consumer: bool) -> Result<(), Error> {
        let msg = encode(&[
            self.channel_id.into_token(),
            self.count.into_token(),
            self.price.into_token(),
            self.is_final.into_token(),
        ]);
        let mut bytes = "\x19Ethereum Signed Message:\n32".as_bytes().to_vec();
        bytes.extend(keccak256(&msg));
        let payload = keccak256(&bytes);
        let sign = key.sign_message(&payload).map_err(|_| Error::InvalidSignature)?;
        if is_consumer {
            self.consumer_sign = sign;
        } else {
            self.indexer_sign = sign;
        }
        Ok(())
    }

    pub fn from_json(params: &Value) -> Result<Self, Error> {
        let channel_id: U256 = params["channelId"]
            .as_str()
            .ok_or(Error::InvalidSerialize)?
            .parse()
            .map_err(|_e| Error::InvalidSerialize)?;
        let indexer: Address = params["indexer"]
            .as_str()
            .ok_or(Error::InvalidSerialize)?
            .parse()
            .map_err(|_e| Error::InvalidSerialize)?;
        let consumer: Address = params["consumer"]
            .as_str()
            .ok_or(Error::InvalidSerialize)?
            .parse()
            .map_err(|_e| Error::InvalidSerialize)?;
        let count = U256::from_dec_str(params["count"].as_str().ok_or(Error::InvalidSerialize)?)
            .map_err(|_e| Error::InvalidSerialize)?;
        let price = U256::from_dec_str(params["price"].as_str().ok_or(Error::InvalidSerialize)?)
            .map_err(|_e| Error::InvalidSerialize)?;
        let is_final = params["isFinal"].as_bool().ok_or(Error::InvalidSerialize)?;
        let indexer_sign: Signature =
            convert_string_to_sign(params["indexerSign"].as_str().ok_or(Error::InvalidSerialize)?);
        let consumer_sign: Signature =
            convert_string_to_sign(params["consumerSign"].as_str().ok_or(Error::InvalidSerialize)?);
        let next_price = U256::from_dec_str(params["nextPrice"].as_str().ok_or(Error::InvalidSerialize)?)
            .map_err(|_e| Error::InvalidSerialize)?;
        Ok(Self {
            channel_id,
            indexer,
            consumer,
            count,
            price,
            is_final,
            indexer_sign,
            consumer_sign,
            next_price,
        })
    }

    pub fn to_json(&self) -> Value {
        json!({
            "channelId": format!("{:#X}", self.channel_id),
            "indexer": format!("{:?}", self.indexer),
            "consumer": format!("{:?}", self.consumer),
            "count": self.count.to_string(),
            "price": self.price.to_string(),
            "isFinal": self.is_final,
            "indexerSign": convert_sign_to_string(&self.indexer_sign),
            "consumerSign": convert_sign_to_string(&self.consumer_sign),
            "nextPrice": self.next_price.to_string(),
        })
    }
}

/// Convert eth signature to string.
pub fn convert_sign_to_string(sign: &Signature) -> String {
    let bytes = convert_sign_to_bytes(sign);
    hex::encode(&bytes)
}

/// Convert string to eth signature.
pub fn convert_string_to_sign(s: &str) -> Signature {
    let mut bytes = hex::decode(s).unwrap_or(vec![0u8; 65]); // 32 + 32 + 1

    if bytes.len() < 65 {
        bytes.extend(vec![0u8; 65 - bytes.len()]);
    }
    let r = H256::from_slice(&bytes[0..32]);
    let s = H256::from_slice(&bytes[32..64]);
    let v = bytes[64] as u64;
    Signature { r, s, v }
}

/// Convert eth signature to bytes.
pub fn convert_sign_to_bytes(sign: &Signature) -> Vec<u8> {
    let mut recovery_id = match sign.v {
        27 => 0,
        28 => 1,
        v if v >= 35 => ((v - 1) % 2) as u8,
        _ => sign.v as u8,
    };
    recovery_id += 27; // Because in ETH.
    let mut bytes = Vec::with_capacity(65);
    bytes.extend_from_slice(sign.r.as_bytes());
    bytes.extend_from_slice(sign.s.as_bytes());
    bytes.push(recovery_id);

    bytes
}

pub fn convert_recovery_sign(sign: &Signature) -> ([u8; 64], i32) {
    let recovery_id = match sign.v {
        27 => 0,
        28 => 1,
        v if v >= 35 => ((v - 1) % 2) as _,
        _ => sign.v as _,
    };
    let signature = {
        let mut sig = [0u8; 64];
        sig[..32].copy_from_slice(sign.r.as_bytes());
        sig[32..].copy_from_slice(sign.s.as_bytes());
        sig
    };
    (signature, recovery_id)
}

pub fn default_sign() -> Signature {
    Signature {
        v: 0,
        r: H256::from([0u8; 32]),
        s: H256::from([0u8; 32]),
    }
}
