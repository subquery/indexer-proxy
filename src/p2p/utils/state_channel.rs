//! state channel contract helper functions.

use rand_chacha::{
    rand_core::{RngCore, SeedableRng},
    ChaChaRng,
};
use secp256k1::{SecretKey, ONE_KEY};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::fs;
use web3::{
    contract::tokens::Tokenizable,
    ethabi::encode,
    signing::{keccak256, recover, Key, SecretKeyRef, Signature, SigningError},
    types::{Address, H256, U256},
};

use crate::p2p::behaviour::rpc::Response;

/// Handle the state channel request/response infos.
pub async fn handle_request(infos: &str) -> Response {
    let key1_path = PathBuf::from("indexer-eth.key");
    let key2_path = PathBuf::from("indexer-controller.key");
    let (indexer_sk, controller_sk) = if key1_path.exists() && key2_path.exists() {
        let key1 = fs::read_to_string(&key1_path).await.unwrap();
        let key2 = fs::read_to_string(&key2_path).await.unwrap();
        let indexer_sk = SecretKey::from_slice(&hex::decode(&key1.trim()).unwrap()).unwrap();
        let controller_sk = SecretKey::from_slice(&hex::decode(&key2.trim()).unwrap()).unwrap();
        (indexer_sk, controller_sk)
    } else {
        let mut rng = ChaChaRng::from_entropy();
        let mut key1_bytes = [0u8; 32];
        let mut key2_bytes = [0u8; 32];
        #[allow(unused_assignments)]
        let mut indexer_sk = ONE_KEY;
        #[allow(unused_assignments)]
        let mut controller_sk = ONE_KEY;
        loop {
            rng.fill_bytes(&mut key1_bytes);
            rng.fill_bytes(&mut key2_bytes);
            let indexer_res = SecretKey::from_slice(&key1_bytes);
            let controller_res = SecretKey::from_slice(&key2_bytes);
            match (indexer_res, controller_res) {
                (Ok(indexer), Ok(controller)) => {
                    indexer_sk = indexer;
                    controller_sk = controller;
                    let _ =
                        fs::write(key1_path, &hex::encode(&indexer_sk.serialize_secret())).await;
                    let _ =
                        fs::write(key2_path, &hex::encode(&controller_sk.serialize_secret())).await;
                    break;
                }
                _ => {}
            }
        }
        (indexer_sk, controller_sk)
    };
    let (indexer, controller) = (
        SecretKeyRef::new(&indexer_sk),
        SecretKeyRef::new(&controller_sk),
    );
    debug!("Indexer: {:?}", indexer.address());
    debug!("Controller: {:?}", controller.address());

    let params = serde_json::from_str::<Value>(infos).unwrap_or(Value::default());
    if params.get("method").is_none() {
        return Response::Error("Invalid request".to_owned());
    }
    match params["method"].as_str().unwrap() {
        "open" => {
            let indexer_address = indexer.address();
            let channel_id: U256 = params["channelId"].as_str().unwrap().parse().unwrap();
            let consumer: Address = params["consumer"].as_str().unwrap().parse().unwrap();
            let amount = U256::from_dec_str(params["amount"].as_str().unwrap()).unwrap();
            let expiration = U256::from_dec_str(params["expiration"].as_str().unwrap()).unwrap();
            let consumer_sign: Signature =
                convert_string_to_sign(params["consumerSign"].as_str().unwrap());
            let (_, indexer_sign) = open_sign(
                Some(channel_id),
                indexer,
                consumer,
                amount,
                expiration,
                Some(&consumer_sign),
            )
            .unwrap();

            let data = json!({
                "channelId": format!("{:#X}", channel_id),
                "indexer": format!("{:?}", indexer_address),
                "consumer": format!("{:?}", consumer),
                "amount": amount.to_string(),
                "expiration": expiration.to_string(),
                "indexerSign": convert_sign_to_string(&indexer_sign),
                "consumerSign": convert_sign_to_string(&consumer_sign),
                "price": U256::from(10i32),
            });

            Response::StateChannel(serde_json::to_string(&data).unwrap())
        }
        "query" => {
            let indexer_address = indexer.address();
            let channel_id: U256 = params["channelId"].as_str().unwrap().parse().unwrap();
            let count = U256::from_dec_str(params["count"].as_str().unwrap()).unwrap();
            let price = U256::from_dec_str(params["price"].as_str().unwrap()).unwrap();
            let is_final = params["isFinal"].as_bool().unwrap();
            let consumer: Address = params["consumer"].as_str().unwrap().parse().unwrap();
            let consumer_sign: Signature =
                convert_string_to_sign(params["consumerSign"].as_str().unwrap());
            let indexer_sign = state_sign(
                channel_id,
                count,
                price,
                is_final,
                indexer, // change to controller
                consumer,
                Some(&consumer_sign),
            )
            .unwrap();

            let data = json!({
                "channelId": format!("{:#X}", channel_id),
                "indexer": format!("{:?}", indexer_address),
                "consumer": format!("{:?}", consumer),
                "count": count.to_string(),
                "price": price.to_string(),
                "isFinal": is_final,
                "indexerSign": convert_sign_to_string(&indexer_sign),
                "consumerSign": convert_sign_to_string(&consumer_sign),
            });

            Response::Sign(serde_json::to_string(&data).unwrap())
        }
        _ => Response::Error("Invalid request".to_owned()),
    }
}

/// Sign the state of the state channel.
pub fn state_sign(
    channel: U256,
    count: U256,
    price: U256,
    is_final: bool,
    key: SecretKeyRef,
    remoter: Address,
    remote_sign: Option<&Signature>,
) -> Result<Signature, SigningError> {
    let msg = encode(&[
        channel.into_token(),
        count.into_token(),
        price.into_token(),
        is_final.into_token(),
    ]);
    let mut bytes = "\x19Ethereum Signed Message:\n32".as_bytes().to_vec();
    bytes.extend(keccak256(&msg));
    let payload = keccak256(&bytes);
    if let Some(remote_sign) = remote_sign {
        let (r_sign, r_id) = convert_recovery_sign(remote_sign);
        let address = recover(&payload, &r_sign, r_id);
        debug!("Signature recover: {:?}, remoter: {:?}", address, remoter);
    }
    key.sign_message(&payload)
}

/// Open and sign a state channel.
pub fn open_sign(
    channel_option: Option<U256>,
    key: SecretKeyRef,
    remoter: Address,
    amount: U256,
    expiration: U256,
    remote_sign: Option<&Signature>,
) -> Result<(U256, Signature), SigningError> {
    let (channel_id, indexer, consumer) = if let Some(channel) = channel_option {
        (channel, key.address(), remoter)
    } else {
        let mut rng = ChaChaRng::from_entropy();
        let mut id = [0u64; 4]; // u256
        for i in 0..4 {
            id[i] = rng.next_u64();
        }
        (U256(id), remoter, key.address())
    };

    let msg = encode(&[
        channel_id.into_token(),
        indexer.into_token(),
        consumer.into_token(),
        amount.into_token(),
        expiration.into_token(),
    ]);
    let mut bytes = "\x19Ethereum Signed Message:\n32".as_bytes().to_vec();
    bytes.extend(keccak256(&msg));
    let payload = keccak256(&bytes);
    if let Some(remote_sign) = remote_sign {
        let (r_sign, r_id) = convert_recovery_sign(remote_sign);
        let address = recover(&payload, &r_sign, r_id);
        debug!("Signature recover: {:?}, remoter: {:?}", address, remoter);
    }
    key.sign_message(&payload).map(|s| (channel_id, s))
}

/// Convert eth signature to string.
pub fn convert_sign_to_string(sign: &Signature) -> String {
    let mut s = String::new();
    s.push_str(&hex::encode(sign.r.as_bytes()));
    s.push_str(&hex::encode(sign.s.as_bytes()));
    s.push_str(&hex::encode(sign.v.to_le_bytes()));
    s
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

/// Convert string to eth signature.
pub fn convert_string_to_sign(s: &str) -> Signature {
    let mut bytes = hex::decode(s).unwrap_or(vec![0u8; 72]); // 36 + 36 + 8
    if bytes.len() < 72 {
        bytes.extend(vec![0u8; 72 - bytes.len()]);
    }
    let r = H256::from_slice(&bytes[0..32]);
    let s = H256::from_slice(&bytes[32..64]);
    let mut v_bytes = [0u8; 8];
    v_bytes.copy_from_slice(&bytes[64..72]);
    let v = u64::from_le_bytes(v_bytes);
    Signature { r, s, v }
}
