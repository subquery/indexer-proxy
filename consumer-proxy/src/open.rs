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

mod cli;

use cli::COMMAND;
use rand_chacha::{
    rand_core::{RngCore, SeedableRng},
    ChaChaRng,
};
use serde_json::json;
use subql_proxy_utils::{payg::convert_sign_to_bytes, request::proxy_request};
use web3::{
    contract::tokens::Tokenizable,
    ethabi::encode,
    signing::{keccak256, Key},
    types::U256,
};

#[tokio::main]
async fn main() {
    let mut rng = ChaChaRng::from_entropy();
    let mut id = [0u64; 4]; // u256
    for i in 0..4 {
        id[i] = rng.next_u64();
    }
    let channel_id = U256(id);
    let amount: U256 = COMMAND.open_amount.parse().unwrap();
    let expiration: U256 = COMMAND.open_expiration.parse().unwrap();

    let signer = COMMAND.signer();
    let consumer = signer.address();

    let msg = encode(&[channel_id.into_token(), amount.into_token(), expiration.into_token()]);
    let mut bytes = "\x19Ethereum Signed Message:\n32".as_bytes().to_vec();
    bytes.extend(keccak256(&msg));
    let payload = keccak256(&bytes);
    let sign = signer.sign_message(&payload).unwrap();

    let data = json!({
        "channelId": format!("{:#X}", channel_id),
        "indexer": COMMAND.open_indexer,
        "amount": amount.to_string(),
        "expiration": expiration.to_string(),
        "consumer": format!("{:?}", consumer),
        "sign": hex::encode(&convert_sign_to_bytes(&sign)),
    });

    let url = format!("http://{}:{}", COMMAND.host(), COMMAND.port());
    let res = proxy_request("post", &url, "open", "", serde_json::to_string(&data).unwrap(), vec![]).await;

    println!("res: {:?}", res);
}
