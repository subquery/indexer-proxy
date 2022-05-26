use serde_json::{json, Value};
use web3::{signing::SecretKeyRef, types::U256};

use crate::account::ACCOUNT;
use crate::p2p::behaviour::rpc::Response;
use crate::payg::{open_state, QueryState, PRICE};

/// Handle the state channel request/response infos.
pub async fn handle(infos: &str) -> Response {
    let params = serde_json::from_str::<Value>(infos).unwrap_or(Value::default());
    if params.get("method").is_none() {
        return Response::Error("Invalid request".to_owned());
    }
    match params["method"].as_str().unwrap() {
        "info" => {
            let account = ACCOUNT.lock().unwrap();
            let data = json!({
                "indexer": format!("{:?}", account.indexer),
                "controller": format!("{:?}", account.controller),
                "price": U256::from(PRICE),
            });
            Response::Sign(serde_json::to_string(&data).unwrap())
        }
        "open" => match open_state(&params).await {
            Ok(state) => Response::Sign(serde_json::to_string(&state).unwrap()),
            Err(err) => Response::Error(err.to_string()),
        },
        "query" => match QueryState::from_json(&params) {
            Ok(mut state) => {
                state.next_price = U256::from(PRICE);
                let account = ACCOUNT.lock().unwrap();
                let key = SecretKeyRef::new(&account.controller_sk);
                match state.sign(key, false) {
                    Err(err) => return Response::Error(err.to_string()),
                    _ => {}
                }
                let _signer = match state.recover() {
                    Ok((_, consumer)) => consumer,
                    Err(err) => return Response::Error(err.to_string()),
                };

                // TODO query state to coordiantor

                Response::Sign(serde_json::to_string(&state.to_json()).unwrap())
            }
            Err(err) => Response::Error(err.to_string()),
        },
        _ => Response::Error("Invalid request".to_owned()),
    }
}
