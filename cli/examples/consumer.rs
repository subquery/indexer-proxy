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
use rustyline::{error::ReadlineError, Editor};
use secp256k1::SecretKey;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env::args;
use std::path::PathBuf;
use subql_proxy_utils::{
    p2p::{libp2p::identity::Keypair, server::server, P2pHandler, Request, Response},
    payg::{convert_sign_to_bytes, default_sign, OpenState, QueryState},
    request::{jsonrpc_request, proxy_request},
};
use web3::{
    api::Eth,
    contract::{
        tokens::{Tokenizable, Tokenize},
        Contract, Options,
    },
    ethabi::{encode, Token},
    signing::{keccak256, Key, SecretKeyRef, Signature},
    transports::Http,
    types::{Address, Bytes, TransactionParameters, U256},
    Web3,
};

fn help() {
    println!("Commands:");
    println!("  help");
    println!("  show");
    println!("  connect [multiaddr] -- eg. connect /ip4/127.0.0.1/tcp/7000");
    println!("  set web3 [web3 endpoint address]");
    println!("    eg. set web3 https://sqtn.api.onfinality.io/public");
    println!("  set contract [contracts.json]");
    println!("  set channel [channel uid]");
    println!("  set indexer [peer-id]");
    println!("  set project [project-id]");
    println!("  state-channel open [indexer] [amount] [expired-seconds]");
    println!("    eg. state-channel open 0x2546bcd3c84621e976d8185a91a922ae77ecec30 100 86400");
    println!("  state-channel checkpoint");
    println!("  state-channel challenge");
    println!("  state-channel claim");
    println!("  state-channel show");
    println!("  state-channel add [channel-id]");
    println!("  query [query]");
    println!("    eg. query query {{ _metadata {{ indexerHealthy chain }} }}");
}

struct StateChannel {
    id: U256,
    count: U256,
    amount: U256,
    _expiration: U256,
    indexer: Address,
    consumer: Address,
    last_price: U256,
    last_final: bool,
    last_indexer_sign: Signature,
    last_consumer_sign: Signature,
    info_indexer: String, // indexer ID
    info_project: String, // project ID
}

pub struct ConsumerP2p;

#[async_trait]
impl P2pHandler for ConsumerP2p {
    async fn request(_request: Request) -> Response {
        todo!()
    }

    async fn event() {
        todo!()
    }
}

fn build_contracts(eth: Eth<Http>, list: Value) -> HashMap<&'static str, Contract<Http>> {
    let mut contracts = HashMap::new();
    for name in vec!["SQToken", "StateChannel", "IndexerRegistry"] {
        contracts.insert(
            name,
            Contract::from_json(
                eth.clone(),
                list[name]["address"].as_str().unwrap().parse().unwrap(),
                &std::fs::read(format!("./examples/contracts/{}.json", name)).unwrap(),
            )
            .unwrap(),
        );
    }
    contracts
}

async fn send_state(
    web3: &Web3<Http>,
    cotract: &Contract<Http>,
    state: &StateChannel,
    method: &str,
    secret: &SecretKey,
) {
    let msg = encode(&[
        state.id.into_token(),
        state.count.into_token(),
        state.last_price.into_token(),
        state.last_final.into_token(),
    ]);
    let mut bytes = "\x19Ethereum Signed Message:\n32".as_bytes().to_vec();
    bytes.extend(keccak256(&msg));
    let _payload = keccak256(&bytes);

    // TODO check sign.
    //let (i_sign, i_id) = convert_recovery_sign(&indexer_sign);
    //let address = recover(&payload, &i_sign, i_id);
    //println!("Recover {:?}", address);

    let call_params = Token::Tuple(vec![
        state.id.into_token(),
        state.last_final.into_token(),
        state.count.into_token(),
        state.last_price.into_token(),
        convert_sign_to_bytes(&state.last_indexer_sign).into_token(),
        convert_sign_to_bytes(&state.last_consumer_sign).into_token(),
    ]);
    let call_tokens = (call_params.clone(),).into_tokens();
    let fn_data = cotract
        .abi()
        .function(method)
        .and_then(|function| function.encode_input(&call_tokens))
        .unwrap();
    let gas = cotract
        .estimate_gas(method, (call_params,), state.consumer, Default::default())
        .await
        .unwrap();

    let tx = TransactionParameters {
        to: Some(cotract.address()),
        data: Bytes(fn_data),
        gas: gas,
        ..Default::default()
    };
    let signed = web3.accounts().sign_transaction(tx, secret).await.unwrap();
    let tx_hash = web3.eth().send_raw_transaction(signed.raw_transaction).await.unwrap();
    println!("\x1b[94m>>> TxHash: {:?}\x1b[00m", tx_hash);
}

const PROXY_URL: &'static str = "http://127.0.0.1:8003";
const PROXY_TOKEN: &'static str = "";
const LOCAL_ENDPOINT: &'static str = "http://127.0.0.1:8545";
const TESTNET_ENDPOINT: &'static str = "https://sqtn.api.onfinality.io/public";

/// Prepare the consumer account and evm status.
/// Run `cargo run --example consumer [local|testnet] [proxy|p2p]` default is local and p2p.
#[tokio::main]
async fn main() {
    let (mut web3_endpoint, net, is_p2p) = if args().len() == 1 {
        (LOCAL_ENDPOINT.to_owned(), "local".to_owned(), true)
    } else {
        if args().len() != 3 {
            println!("cargo run --example consumer [local|testnet] [proxy|p2p]");
            return;
        }
        let (endpoint, net) = if args().nth(1).unwrap() == "local".to_owned() {
            (LOCAL_ENDPOINT.to_owned(), "local".to_owned())
        } else {
            (TESTNET_ENDPOINT.to_owned(), "testnet".to_owned())
        };
        let is_p2p = if args().nth(2).unwrap() == "proxy".to_owned() {
            false
        } else {
            true
        };
        (endpoint, net, is_p2p)
    };

    // default test consumer secret key. (same with prepare.rs)
    let consumer_str = "de9be858da4a475276426320d5e9262ecfc3ba460bfac56360bfa6c4c28b4ee0";
    let default_indexer = "12D3KooWSvjBEHfxQVcMSfSNAAjSr2uGXJv6RfFYGiYQmWcY2opm";
    let default_project = "QmYR8xQgAXuCXMPGPVxxR91L4VtKZsozCM7Qsa5oAbyaQ3";

    // consumer/controller eth account (PROD need Keystore).
    let consumer_sk = SecretKey::from_slice(&hex::decode(&consumer_str).unwrap()).unwrap();
    let consumer_ref = SecretKeyRef::new(&consumer_sk);
    let consumer = consumer_ref.address();

    let mut current_indexer: String = String::from(default_indexer);
    let mut current_project: String = String::from(default_project);

    // init web3
    let http = Http::new(&web3_endpoint).unwrap();
    let mut web3 = Web3::new(http);
    if !PathBuf::from(format!("./examples/contracts/{}.json", net)).exists() {
        println!("Missing contracts deployment. See contracts repo public/{}.json", net);
        return;
    }
    let file = std::fs::File::open(format!("./examples/contracts/{}.json", net)).unwrap();
    let reader = std::io::BufReader::new(file);
    let list = serde_json::from_reader(reader).unwrap();
    let mut contracts = build_contracts(web3.eth(), list);

    // cid => StateChannel
    let mut channels: Vec<StateChannel> = vec![];
    let mut cid: usize = 0;

    // local p2p rpc bind.
    let url = "http://127.0.0.1:7777";
    if is_p2p {
        let key_bytes = hex::decode("0801124021220100bdf8d7da7c51e1e76724bb0f1001d4dbf621662d4fab121a908868bbfe37eab62abbd576faabe024d0a19566a20108a4a29c8bc25184c4d5a6e05782").unwrap();
        let p2p_key = Keypair::from_protobuf_encoding(&key_bytes).unwrap();

        tokio::spawn(async move {
            server::<ConsumerP2p>(
                "/ip4/0.0.0.0/tcp/0".parse().unwrap(),
                "127.0.0.1:7777".parse().unwrap(),
                None,
                None,
                p2p_key,
            )
            .await
            .unwrap();
        });
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            jsonrpc_request(0, url, "connect", vec![json!("/ip4/127.0.0.1/tcp/7000")]).await
        });
    }

    println!("START QUERY, please input indexer's PeerId!");
    help();

    // Read full lines from stdin
    let mut rl = Editor::<()>::new();
    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }
    loop {
        println!("\x1b[92m------------------------------------\x1b[00m");
        let readline = rl.readline(">> ");
        let line = match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str());
                line
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        };
        let method_params = line.trim().split_once(" ");
        if method_params.is_none() {
            match line.as_str() {
                "help" => help(),
                "show" => {
                    println!("Account Consumer:       {:?}", consumer);
                    //println!("Account Controller:     {:?}", controller.address());
                    println!("State Channel Contract: {}", contracts["StateChannel"].address());
                    println!("Web3 Endpoint:          {}", web3_endpoint);
                    println!("");
                    if channels.len() == 0 {
                        println!("Current Channel: None");
                    } else {
                        println!("Current Channel: {} {:#X}", cid, channels[cid].id);
                    }
                    println!("Default indexer: {}", current_indexer);
                    println!("Default project: {}", current_project);
                    let result: U256 = contracts["SQToken"]
                        .query("balanceOf", (consumer,), None, Options::default(), None)
                        .await
                        .unwrap();
                    println!("SQT Balance: {:?}", result);
                }
                _ => println!("\x1b[91mInvalid, type again!\x1b[00m"),
            }
            continue;
        }
        let (method, params) = method_params.unwrap();
        let params = params.trim().to_owned();
        match method {
            "connect" => {
                if !is_p2p {
                    println!("\x1b[91m>>> Only P2P supported\x1b[00m");
                }
                if jsonrpc_request(0, url, "connect", vec![Value::from(params.clone())])
                    .await
                    .is_ok()
                {
                    println!("\x1b[93m>>> Start connect to: {}\x1b[00m", params);
                } else {
                    println!("\x1b[91m>>> Invalid Params\x1b[00m");
                }
            }
            "set" => {
                let method_params = params.split_once(" ");
                if method_params.is_none() {
                    println!("\x1b[91mInvalid, type again!\x1b[00m");
                    continue;
                }
                let (method, params) = method_params.unwrap();
                let params = params.trim().to_owned();
                match method {
                    "web3" => match Http::new(&params) {
                        Ok(http) => {
                            web3_endpoint = params;
                            web3 = Web3::new(http);
                            println!("\x1b[93m>>> Web3 changed to: {}\x1b[00m", web3_endpoint);
                        }
                        Err(err) => {
                            println!("\x1b[91m>>> Error: {}\x1b[00m", err);
                        }
                    },
                    "contract" => {
                        let file = std::fs::File::open(params).unwrap();
                        let reader = std::io::BufReader::new(file);
                        let list = serde_json::from_reader(reader).unwrap();
                        contracts = build_contracts(web3.eth(), list);
                        println!(
                            "\x1b[93m>>> Contract changed to: {}\x1b[00m",
                            contracts["StateChannel"].address()
                        );
                    }
                    "channel" => {
                        cid = params.parse().unwrap();
                        println!(
                            "\x1b[93m>>> Channel changed to: {} {:#X}\x1b[00m",
                            cid, channels[cid].id,
                        );
                    }
                    "indexer" => {
                        current_indexer = params;
                        println!("\x1b[93m>>> Indexer changed to: {}\x1b[00m", current_indexer);
                    }
                    "project" => {
                        current_project = params;
                        println!("\x1b[93m>>> Project changed to: {}\x1b[00m", current_project);
                    }
                    _ => println!("\x1b[91mInvalid, type again!\x1b[00m"),
                }
            }
            "state-channel" => {
                let method_params = params.split_once(" ");
                let (method, params) = if method_params.is_none() {
                    (params.as_str(), "".to_owned())
                } else {
                    let (method, params) = method_params.unwrap();
                    let params = params.trim().to_owned();
                    (method, params)
                };
                if channels.len() == 0 && method != "open" && method != "add" {
                    println!("\x1b[91mNo Channel, please open or add!\x1b[00m");
                    continue;
                }
                match method {
                    "open" => {
                        let mut next_params = params.split(" ");
                        let indexer: Address = next_params.next().unwrap().parse().unwrap();
                        let amount = U256::from_dec_str(next_params.next().unwrap()).unwrap();
                        let expiration = U256::from_dec_str(next_params.next().unwrap()).unwrap();
                        let deployment_id = bs58::decode(default_project).into_vec().unwrap();

                        let state = OpenState::consumer_generate(
                            None,
                            indexer,
                            consumer,
                            amount,
                            expiration,
                            deployment_id,
                            vec![],
                            SecretKeyRef::new(&consumer_sk),
                        )
                        .unwrap();
                        let raw_state = serde_json::to_string(&state.to_json()).unwrap();

                        let res = if is_p2p {
                            let data = json!({ "method": "open", "state": raw_state });
                            let infos = serde_json::to_string(&data).unwrap();
                            let query = vec![Value::from(current_indexer.as_str()), Value::from(infos)];
                            jsonrpc_request(0, url, "state-channel", query).await
                        } else {
                            proxy_request("post", PROXY_URL, "open", PROXY_TOKEN, raw_state, vec![]).await
                        };

                        match res {
                            Ok(data) => {
                                let state = OpenState::from_json(&data).unwrap();
                                println!("channelId:  {:#X}", state.channel_id);
                                println!("amount:     {}", state.amount);
                                println!("expiration: {}", state.expiration);
                                println!("indexer:    {:?}", state.indexer);
                                println!("consumer:   {:?}", state.consumer);

                                cid = channels.len();
                                channels.push(StateChannel {
                                    id: state.channel_id,
                                    count: U256::from(0u64),
                                    amount: state.amount,
                                    _expiration: state.expiration,
                                    indexer: state.indexer,
                                    consumer: state.consumer,
                                    last_price: state.next_price,
                                    last_final: false,
                                    last_indexer_sign: state.indexer_sign,
                                    last_consumer_sign: state.consumer_sign,
                                    info_indexer: current_indexer.clone(),
                                    info_project: current_project.clone(),
                                });
                            }
                            Err(err) => println!("\x1b[91m>>> Error: {}\x1b[00m", err),
                        }
                    }
                    "checkpoint" => {
                        send_state(
                            &web3,
                            &contracts["StateChannel"],
                            &channels[cid],
                            "checkpoint",
                            &consumer_sk,
                        )
                        .await;
                    }
                    "challenge" => {
                        send_state(
                            &web3,
                            &contracts["StateChannel"],
                            &channels[cid],
                            "challenge",
                            &consumer_sk,
                        )
                        .await;
                    }
                    "respond" => {
                        send_state(
                            &web3,
                            &contracts["StateChannel"],
                            &channels[cid],
                            "respond",
                            &consumer_sk,
                        )
                        .await;
                    }
                    "claim" => {
                        let channel_id = channels[cid].id;
                        let fn_data = contracts["StateChannel"]
                            .abi()
                            .function("claim")
                            .and_then(|function| function.encode_input(&(channel_id,).into_tokens()))
                            .unwrap();
                        let gas = contracts["StateChannel"]
                            .estimate_gas("claim", (channel_id,), channels[cid].consumer, Default::default())
                            .await;
                        if gas.is_err() {
                            println!("Channel not expired");
                            continue;
                        }
                        let gas = gas.unwrap();
                        let tx = TransactionParameters {
                            to: Some(contracts["StateChannel"].address()),
                            data: Bytes(fn_data),
                            gas: gas,
                            ..Default::default()
                        };
                        let signed = web3.accounts().sign_transaction(tx, &consumer_sk).await.unwrap();
                        let tx_hash = web3.eth().send_raw_transaction(signed.raw_transaction).await.unwrap();
                        println!("\x1b[94m>>> TxHash: {:?}\x1b[00m", tx_hash);
                    }
                    "show" => {
                        let result: (Token,) = contracts["StateChannel"]
                            .query("channel", (channels[cid].id,), None, Options::default(), None)
                            .await
                            .unwrap();
                        match result.0 {
                            Token::Tuple(data) => {
                                let count: U256 = data[3].clone().into_uint().unwrap().into();
                                let amount: U256 = data[4].clone().into_uint().unwrap().into();
                                let expiration: U256 = data[5].clone().into_uint().unwrap().into();
                                println!("State Channel Status: {}", data[0]);
                                println!(" Indexer:  0x{}", data[1]);
                                println!(" Consumer: 0x{}", data[2]);
                                println!(" Count On-chain: {:?}, Now: {}", count, channels[cid].count);
                                println!(" Amount:         {:?}", amount);
                                println!(" Expiration:     {:?}", expiration);
                            }
                            _ => {}
                        }
                    }
                    "add" => {
                        let channel_id: U256 = params.parse().unwrap();
                        let result: (Token,) = contracts["StateChannel"]
                            .query("channel", (channel_id,), None, Options::default(), None)
                            .await
                            .unwrap();
                        match result.0 {
                            Token::Tuple(data) => {
                                let count: U256 = data[3].clone().into_uint().unwrap().into();
                                let amount: U256 = data[4].clone().into_uint().unwrap().into();
                                let expiration: U256 = data[5].clone().into_uint().unwrap().into();
                                println!("State Channel Status: {}", data[0]);
                                println!(" Indexer:  0x{}", data[1]);
                                println!(" Consumer: 0x{}", data[2]);
                                println!(" On-chain Count:  {}", count);
                                println!(" Amount:          {}", amount);
                                println!(" Expiration:      {}", expiration);
                                cid = channels.len();
                                channels.push(StateChannel {
                                    id: channel_id,
                                    count: count,
                                    amount: amount,
                                    _expiration: expiration,
                                    indexer: data[1].clone().into_address().unwrap(),
                                    consumer: data[2].clone().into_address().unwrap(),
                                    last_price: U256::from(10u64),
                                    last_final: false,
                                    last_indexer_sign: default_sign(),
                                    last_consumer_sign: default_sign(),
                                    info_indexer: current_indexer.clone(),
                                    info_project: current_project.clone(),
                                });
                            }
                            _ => {}
                        }
                    }
                    _ => println!("\x1b[91mInvalid, type again!\x1b[00m"),
                }
            }
            "query" => {
                let mut data = HashMap::new();
                data.insert("query", params);

                if channels.len() == 0 {
                    println!("\x1b[91mNo Channel, please open or add Channel!\x1b[00m");
                    continue;
                }

                let is_final = channels[cid].count * channels[cid].last_price >= channels[cid].amount;
                let next_count = channels[cid].count + U256::from(1u64);
                println!("Next count: {}", next_count);
                let state = QueryState::consumer_generate(
                    channels[cid].id,
                    channels[cid].indexer,
                    channels[cid].consumer,
                    next_count,
                    channels[cid].last_price,
                    is_final,
                    SecretKeyRef::new(&consumer_sk),
                )
                .unwrap();
                let raw_query = serde_json::to_string(&data).unwrap();
                let raw_state = serde_json::to_string(&state.to_json()).unwrap();
                let res = if is_p2p {
                    let query = vec![
                        Value::from(channels[cid].info_indexer.as_str()),
                        Value::from(channels[cid].info_project.as_str()),
                        Value::from(raw_query),
                        Value::from(raw_state),
                    ];

                    jsonrpc_request(0, url, "payg-sync", query).await
                } else {
                    proxy_request(
                        "post",
                        PROXY_URL,
                        &format!("payg/{}", channels[cid].info_project),
                        PROXY_TOKEN,
                        raw_query,
                        vec![("Authorization".to_owned(), raw_state)],
                    )
                    .await
                };
                match res {
                    Ok(fulldata) => {
                        let (query, data) = (&fulldata[0], &fulldata[1]);
                        println!("\x1b[94m>>> Result: {}\x1b[00m", query);
                        let state = QueryState::from_json(&data).unwrap();

                        channels[cid].count = state.count;
                        channels[cid].last_price = state.next_price;
                        channels[cid].last_final = state.is_final;
                        channels[cid].last_indexer_sign = state.indexer_sign;
                        channels[cid].last_consumer_sign = state.consumer_sign;

                        if state.count % U256::from(5u64) == U256::from(0u64) {
                            println!("Every 5 times will auto checkpoint...");
                            send_state(
                                &web3,
                                &contracts["StateChannel"],
                                &channels[cid],
                                "checkpoint",
                                &consumer_sk,
                            )
                            .await;
                        }
                    }
                    Err(err) => println!("\x1b[91m>>> Error: {}\x1b[00m", err),
                }
            }
            _ => {
                println!("\x1b[91mInvalid, type again!\x1b[00m");
            }
        }
    }
    rl.save_history("history.txt").unwrap();
}
