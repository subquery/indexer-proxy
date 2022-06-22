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

use rand_chacha::{
    rand_core::{RngCore, SeedableRng},
    ChaChaRng,
};
use secp256k1::SecretKey;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use structopt::StructOpt;
use subql_proxy_utils::{
    payg::convert_sign_to_bytes,
    request::{graphql_request, proxy_request},
};
use web3::{
    contract::{
        tokens::{Tokenizable, Tokenize},
        Contract, Options,
    },
    ethabi::{encode, Token},
    signing::{keccak256, Key, SecretKeyRef},
    transports::Http,
    types::{Address, Bytes, TransactionParameters, U256},
    Web3,
};

//const LOCAL_ENDPOINT: &'static str = "http://127.0.0.1:8545";
//const TESTNET_ENDPOINT: &'static str = "https://sqtn.api.onfinality.io/public";
const SLEEP: u64 = 2;
const COORDINATOR_URL: &'static str = "http://127.0.0.1:8000/graphql";
const CONSUMER_PROXY: &'static str = "http://127.0.0.1:8010";

// Init mnemonic: test test test test test test test test test test test junk
const MINER: &'static str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const INDEXER: &'static str = "ea6c44ac03bff858b476bba40716402b03e41b8e97e276d1baec7c37d42484a0";
const CONTROLLER: &'static str = "689af8efa8c651a91ad287602527f3af2fe9f6501a7ac4b061667b5a93e037fd";
const CONSUMER: &'static str = "de9be858da4a475276426320d5e9262ecfc3ba460bfac56360bfa6c4c28b4ee0";

/// Command of the consumer and indexer script.
/// Run `cargo run`
#[derive(Debug, StructOpt)]
#[structopt(about = "the command scripts for consumer & indexer")]
enum Cli {
    /// Auto-script to prepare indexer and consumer.
    Auto {
        #[structopt(short, long)]
        endpoint: String,
        #[structopt(short, long)]
        deploy: String,
        #[structopt(short, long)]
        contracts: String,
    },
    /// Register a indexer.
    IndexerRegister {
        #[structopt(short, long)]
        endpoint: String,
        #[structopt(short, long)]
        deploy: String,
        #[structopt(short, long)]
        contracts: String,
    },
    /// Register a consumer to Consumer hoster.
    ConsumerRegister {
        #[structopt(short, long)]
        endpoint: String,
        #[structopt(short, long)]
        deploy: String,
        #[structopt(short, long)]
        contracts: String,
    },
    /// Open a state channel for consumer proxy.
    ConsumerOpen {
        #[structopt(short, long)]
        amount: u128,
        #[structopt(short, long)]
        expiration: u128,
        #[structopt(short, long)]
        deployment: String,
    },
    /// Channel show on-chain info.
    ChannelShow {
        #[structopt(short, long)]
        endpoint: String,
        #[structopt(short, long)]
        deploy: String,
        #[structopt(short, long)]
        contracts: String,
        #[structopt(short, long)]
        id: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::from_args();
    println!("{:?}", cli);
    match cli {
        Cli::Auto {
            endpoint,
            deploy,
            contracts,
        } => {
            let (web3, contracts, miner, indexer, controller, consumer) =
                init(endpoint, deploy, contracts, true).await.unwrap();
            // Transfer DEV main token to indexer/consumer
            let indexer_addr = SecretKeyRef::new(&indexer).address();
            let consumer_addr = SecretKeyRef::new(&consumer).address();
            transfer(&web3, &miner, indexer_addr, 1_000_000_000_000_000_000).await;
            transfer(&web3, &miner, consumer_addr, 1_000_000_000_000_000_000).await;

            println!("\x1b[92m------------------------------------\x1b[00m");
            // Transfer SQT to indexer/consumer
            transfer_token(&web3, &contracts["SQToken"], &miner, indexer_addr, 1000000).await;
            transfer_token(&web3, &contracts["SQToken"], &miner, consumer_addr, 1000000).await;

            println!("\x1b[92m------------------------------------\x1b[00m");
            // Register indexer
            let staking = contracts["Staking"].address();
            let channel = contracts["StateChannel"].address();
            let token_c = &contracts["SQToken"];
            token_approve(&web3, token_c, &indexer, staking, u128::MAX).await;
            token_approve(&web3, token_c, &consumer, channel, u128::MAX).await;

            register_indexer(&web3, &contracts["IndexerRegistry"], &indexer, &controller, 100000).await;
            register_consumer_proxy(&web3, &contracts, &miner, &consumer, 1000).await;
        }
        Cli::IndexerRegister {
            endpoint,
            deploy,
            contracts,
        } => {
            let (web3, contracts, miner, indexer, controller, _consumer) =
                init(endpoint, deploy, contracts, false).await.unwrap();
            let staking = contracts["Staking"].address();
            let indexer_addr = SecretKeyRef::new(&indexer).address();
            transfer_token(&web3, &contracts["SQToken"], &miner, indexer_addr, 1000000).await;
            token_approve(&web3, &contracts["SQToken"], &indexer, staking, u128::MAX).await;
            register_indexer(&web3, &contracts["IndexerRegistry"], &indexer, &controller, 100000).await;
        }
        Cli::ConsumerRegister {
            endpoint,
            deploy,
            contracts,
        } => {
            let (web3, contracts, miner, _indexer, _controller, consumer) =
                init(endpoint, deploy, contracts, false).await.unwrap();
            register_consumer_proxy(&web3, &contracts, &miner, &consumer, 1000).await;
        }
        Cli::ConsumerOpen {
            amount,
            expiration,
            deployment,
        } => {
            let consumer = SecretKey::from_slice(&hex::decode(CONSUMER).unwrap()).unwrap();
            let indexer = SecretKey::from_slice(&hex::decode(INDEXER).unwrap()).unwrap();
            let indexer_addr = SecretKeyRef::new(&indexer).address();
            open_channel_with_consumer(&consumer, indexer_addr, amount, expiration, deployment).await;
        }
        Cli::ChannelShow {
            endpoint,
            deploy,
            contracts,
            id,
        } => {
            let id: U256 = id.parse().unwrap();
            let (_web3, contracts, _miner, _indexer, _controller, _consumer) =
                init(endpoint, deploy, contracts, false).await.unwrap();
            let result: (Token,) = contracts["StateChannel"]
                .query("channel", (id,), None, Options::default(), None)
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
                    println!(" Count On-chain: {:?}", count);
                    println!(" Amount:         {:?}", amount);
                    println!(" Expiration:     {:?}", expiration);
                }
                _ => {}
            }
        }
    }
}

async fn init(
    endpoint: String,
    deploy_path: String,
    contract_path: String,
    show: bool,
) -> Result<
    (
        Web3<Http>,
        HashMap<&'static str, Contract<Http>>,
        SecretKey,
        SecretKey,
        SecretKey,
        SecretKey,
    ),
    (),
> {
    let miner = SecretKey::from_slice(&hex::decode(MINER).unwrap()).unwrap();
    let indexer = SecretKey::from_slice(&hex::decode(INDEXER).unwrap()).unwrap();
    let controller = SecretKey::from_slice(&hex::decode(CONTROLLER).unwrap()).unwrap();
    let consumer = SecretKey::from_slice(&hex::decode(CONSUMER).unwrap()).unwrap();

    let web3 = Web3::new(Http::new(&endpoint).unwrap());
    if !PathBuf::from(&deploy_path).exists() {
        println!("Missing contracts deployment. See contracts repo public/mainnet|testnet|local.json");
        return Err(());
    }
    let file = std::fs::File::open(deploy_path).unwrap();
    let reader = std::io::BufReader::new(file);
    let list: serde_json::Value = serde_json::from_reader(reader).unwrap();
    let mut contracts = HashMap::new();
    for name in vec![
        "SQToken",
        "StateChannel",
        "IndexerRegistry",
        "Staking",
        "ConsumerProxy",
        "ConsumerHoster",
    ] {
        let file = std::fs::File::open(format!("{}/{}.sol/{}.json", contract_path, name, name)).unwrap();
        let reader = std::io::BufReader::new(file);
        let contract: serde_json::Value = serde_json::from_reader(reader).unwrap();

        contracts.insert(
            name,
            Contract::from_json(
                web3.eth(),
                list[name]["address"].as_str().unwrap().parse().unwrap(),
                &serde_json::to_string(&contract["abi"]).unwrap().as_bytes(),
            )
            .unwrap(),
        );
    }

    if show {
        let miner_addr = SecretKeyRef::new(&miner).address();
        let result: String = contracts["SQToken"]
            .query("symbol", (), None, Options::default(), None)
            .await
            .unwrap();
        println!("Token Symbol: {:?}", result);
        let result: Address = contracts["SQToken"]
            .query("getMinter", (), None, Options::default(), None)
            .await
            .unwrap();
        println!("Token Miner: {:?} != {:?}", result, miner_addr);
        let result: U256 = web3.eth().balance(miner_addr, None).await.unwrap();
        println!("Miner Balance: {:?}", result);

        let result: U256 = contracts["SQToken"]
            .query("balanceOf", (miner_addr,), None, Options::default(), None)
            .await
            .unwrap();
        println!("Miner SQT Balance: {:?}", result);

        println!("\x1b[92m------------------------------------\x1b[00m");
    }
    Ok((web3, contracts, miner, indexer, controller, consumer))
}

async fn transfer(web3: &Web3<Http>, sk: &SecretKey, address: Address, amount: u128) {
    println!("Transfer FEE to: {:?} ...", address);
    let tx = TransactionParameters {
        to: Some(address),
        value: U256::from(amount),
        ..Default::default()
    };
    let signed = web3.accounts().sign_transaction(tx, sk).await.unwrap();
    let _tx_hash = web3.eth().send_raw_transaction(signed.raw_transaction).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(SLEEP)).await;
    let result: U256 = web3.eth().balance(address, None).await.unwrap();
    println!("{:?} Balance: {:?}", address, result);
}

async fn transfer_token(web3: &Web3<Http>, contract: &Contract<Http>, sk: &SecretKey, address: Address, amount: u128) {
    println!("Transfer SQT to: {:?} ...", address);
    let fn_data = contract
        .abi()
        .function("transfer")
        .and_then(|function| function.encode_input(&(address, U256::from(amount)).into_tokens()))
        .unwrap();
    let tx = TransactionParameters {
        to: Some(contract.address()),
        data: Bytes(fn_data),
        ..Default::default()
    };
    let signed = web3.accounts().sign_transaction(tx, sk).await.unwrap();
    let _tx_hash = web3.eth().send_raw_transaction(signed.raw_transaction).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(SLEEP)).await;
    let result: U256 = contract
        .query("balanceOf", (address,), None, Options::default(), None)
        .await
        .unwrap();
    println!("{:?} SQT Balance: {:?}", address, result);
}

async fn token_approve(web3: &Web3<Http>, contract: &Contract<Http>, sk: &SecretKey, address: Address, amount: u128) {
    println!("Approve SQT to: {:?} ...", address);
    let fn_data = contract
        .abi()
        .function("increaseAllowance")
        .and_then(|function| function.encode_input(&(address, U256::from(amount)).into_tokens()))
        .unwrap();
    let tx = TransactionParameters {
        to: Some(contract.address()),
        data: Bytes(fn_data),
        ..Default::default()
    };
    let signed = web3.accounts().sign_transaction(tx, sk).await.unwrap();
    let _tx_hash = web3.eth().send_raw_transaction(signed.raw_transaction).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(SLEEP)).await;
    let result: U256 = contract
        .query(
            "allowance",
            (SecretKeyRef::new(sk).address(), address),
            None,
            Options::default(),
            None,
        )
        .await
        .unwrap();
    println!("Approved SQT {:?}", result);
}

async fn register_indexer(
    web3: &Web3<Http>,
    contract: &Contract<Http>,
    sk: &SecretKey,
    controller: &SecretKey,
    amount: u128,
) {
    let indexer = SecretKeyRef::new(&sk);
    let address = indexer.address();
    println!("Register Indexer: {:?} ...", indexer.address());
    let result: bool = contract
        .query("isIndexer", (address,), None, Options::default(), None)
        .await
        .unwrap();
    if result {
        println!("Had Register Indexer: {}", result);
    } else {
        let gas = contract
            .estimate_gas(
                "registerIndexer",
                (U256::from(amount), [0u8; 32], U256::from(0i32)),
                address,
                Default::default(),
            )
            .await
            .unwrap();
        let fn_data = contract
            .abi()
            .function("registerIndexer")
            .and_then(|function| {
                function.encode_input(&(U256::from(amount), [0u8; 32], U256::from(0i32)).into_tokens())
            })
            .unwrap();
        //let nonce = web3.eth().transaction_count(address, None).await.unwrap();
        let tx = TransactionParameters {
            to: Some(contract.address()),
            data: Bytes(fn_data),
            gas: gas,
            ..Default::default()
        };

        let signed = web3.accounts().sign_transaction(tx, sk).await.unwrap();
        let _tx_hash = web3.eth().send_raw_transaction(signed.raw_transaction).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_secs(SLEEP)).await;
        let result: bool = contract
            .query("isIndexer", (address,), None, Options::default(), None)
            .await
            .unwrap();
        println!("On-chain Indexer: {}", result);
    }

    println!("Save Indexer to coordinator...");

    let mdata = format!(
        r#"mutation {{
  addIndexer(indexer:"{:?}") {{
    indexer
  }}
}}
"#,
        address
    );
    let query = json!({ "query": mdata });
    let res = graphql_request(COORDINATOR_URL, &query).await.unwrap();
    println!("Coordinator result: {}", res);
    println!("Register Indexer OK");

    let controller_addr = SecretKeyRef::new(controller).address();
    println!("Register Controller: {:?} ...", controller_addr);
    let controller_chain: Address = contract
        .query("indexerToController", (address,), None, Options::default(), None)
        .await
        .unwrap();
    if controller_chain == controller_addr {
        println!("Had Register Controller: {:?}", controller_addr);
    } else {
        let gas = contract
            .estimate_gas("setControllerAccount", (controller_addr,), address, Default::default())
            .await
            .unwrap();
        let fn_data = contract
            .abi()
            .function("setControllerAccount")
            .and_then(|function| function.encode_input(&(controller_addr,).into_tokens()))
            .unwrap();
        let tx = TransactionParameters {
            to: Some(contract.address()),
            data: Bytes(fn_data),
            gas: gas,
            ..Default::default()
        };

        let signed = web3.accounts().sign_transaction(tx, sk).await.unwrap();
        let _tx_hash = web3.eth().send_raw_transaction(signed.raw_transaction).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_secs(SLEEP)).await;
        let result: Address = contract
            .query("indexerToController", (address,), None, Options::default(), None)
            .await
            .unwrap();
        println!("On-chain Controller: {}", result);
    }

    let mdata = format!(
        r#"mutation {{
  updateController(controller:"0x{}") {{
    controller
  }}
}}
"#,
        format!("{}", controller.display_secret())
    );
    let query = json!({ "query": mdata });
    graphql_request(COORDINATOR_URL, &query).await.unwrap();
    println!("Register Controller OK");
}

async fn register_consumer_proxy(
    web3: &Web3<Http>,
    contracts: &HashMap<&str, Contract<Http>>,
    miner_sk: &SecretKey,
    consumer_sk: &SecretKey,
    amount: u128,
) {
    let contract = &contracts["ConsumerProxy"];
    let sqtoken = &contracts["SQToken"];
    let miner = SecretKeyRef::new(&miner_sk);
    let consumer = SecretKeyRef::new(&consumer_sk);
    let address = consumer.address();
    let miner_addr = miner.address();

    let result: Address = contract
        .query("signer", (), None, Options::default(), None)
        .await
        .unwrap();
    if result == miner_addr {
        println!("Signer had registered");
    } else {
        println!("Register signer: {:?} ...", miner_addr);
        let gas = contract
            .estimate_gas("setSigner", (miner_addr,), miner_addr, Default::default())
            .await
            .unwrap();
        let fn_data = contract
            .abi()
            .function("setSigner")
            .and_then(|function| function.encode_input(&(miner_addr,).into_tokens()))
            .unwrap();

        let tx = TransactionParameters {
            to: Some(contract.address()),
            data: Bytes(fn_data),
            gas: gas,
            ..Default::default()
        };

        let signed = web3.accounts().sign_transaction(tx, miner_sk).await.unwrap();
        let _tx_hash = web3.eth().send_raw_transaction(signed.raw_transaction).await.unwrap();
        println!("Register signer ok");
    }

    let result: Address = contract
        .query("consumer", (), None, Options::default(), None)
        .await
        .unwrap();
    if result == address {
        println!("Consumer had registered");
        return;
    }

    println!("Transfer SQT to contract...");
    transfer_token(web3, sqtoken, consumer_sk, contract.address(), amount).await;

    println!("Register consumer: {:?} ...", address);
    let gas = contract
        .estimate_gas("setConsumer", (address,), miner.address(), Default::default())
        .await
        .unwrap();
    let fn_data = contract
        .abi()
        .function("setConsumer")
        .and_then(|function| function.encode_input(&(address,).into_tokens()))
        .unwrap();

    let tx = TransactionParameters {
        to: Some(contract.address()),
        data: Bytes(fn_data),
        gas: gas,
        ..Default::default()
    };

    let signed = web3.accounts().sign_transaction(tx, miner_sk).await.unwrap();
    let _tx_hash = web3.eth().send_raw_transaction(signed.raw_transaction).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(SLEEP)).await;
    let result: Address = contract
        .query("consumer", (), None, Options::default(), None)
        .await
        .unwrap();
    println!("On-chain Consumer: {}", result == address);
}

async fn open_channel_with_consumer(
    sk: &SecretKey,
    indexer: Address,
    amount: u128,
    expiration: u128,
    deployment: String,
) {
    let consumer = SecretKeyRef::new(sk).address();
    let mut rng = ChaChaRng::from_entropy();
    let mut id = [0u64; 4]; // u256
    for i in 0..4 {
        id[i] = rng.next_u64();
    }
    let channel = U256(id);
    let amount = U256::from(amount);
    let expiration = U256::from(expiration);

    let deployment_id = if deployment.starts_with("0x") {
        hex::decode(&deployment[2..]).unwrap()
    } else {
        // default is bs58
        bs58::decode(deployment).into_vec().unwrap()
    };
    if deployment_id.len() != 32 {
        println!("Invalid deployment(project) id!");
        return;
    }

    let msg = encode(&[channel.into_token(), amount.into_token()]);
    let mut bytes = "\x19Ethereum Signed Message:\n32".as_bytes().to_vec();
    bytes.extend(keccak256(&msg));
    let payload = keccak256(&bytes);
    let sign = sk.sign_message(&payload).unwrap();
    let callback = hex::encode(convert_sign_to_bytes(&sign));

    let query = json!({
        "channelId": format!("{:#X}", channel),
        "indexer": format!("{:?}", indexer),
        "consumer": format!("{:?}", consumer),
        "amount": amount.to_string(),
        "expiration": expiration.to_string(),
        "deploymentId": hex::encode(deployment_id),
        "sign": callback,
    });
    let data = serde_json::to_string(&query).unwrap();
    let res = proxy_request("POST", CONSUMER_PROXY, "open", "", data, vec![]).await;
    match res {
        Ok(res) => println!("Success: {}", res),
        Err(res) => println!("Failure: {}", res),
    }
}
