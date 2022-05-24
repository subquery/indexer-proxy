use serde_json::{json, Value};

use crate::p2p::behaviour::rpc::Response;
use crate::request::{graphql_request, REQUEST_CLIENT};

pub async fn proxy_request(url: String, query: String) -> Response {
    let res = graphql_request(&url, &json!({ "query": query })).await;
    match res {
        Ok(value) => match value.pointer("/data") {
            Some(data) => Response::RawData(serde_json::to_string(data).unwrap()), // unwrap safe
            _ => Response::Error("Data is missing".to_owned()),
        },
        Err(err) => Response::Error(err.to_string()),
    }
}

pub async fn jsonrpc_request(
    id: u64,
    url: &str,
    method: &str,
    params: Vec<Value>,
) -> Result<Value, Value> {
    let res = REQUEST_CLIENT
        .post(url)
        .header("content-type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))
        .send()
        .await
        .unwrap();

    match res.error_for_status() {
        Ok(res) => match res.json::<Value>().await {
            Ok(data) => {
                if data.get("result").is_some() {
                    if data["result"].is_array() {
                        let mut res = vec![];
                        for i in data["result"].as_array().unwrap() {
                            let i_str = i.as_str().unwrap();
                            match serde_json::from_str::<Value>(i_str) {
                                Ok(r) => res.push(r),
                                Err(_) => res.push(Value::from(i_str)),
                            }
                        }
                        Ok(json!(res))
                    } else {
                        let res = data["result"].as_str().unwrap_or("");
                        if let Ok(json) = serde_json::from_str::<Value>(res) {
                            if json.get("errors").is_some() {
                                Err(json)
                            } else {
                                Ok(json)
                            }
                        } else {
                            Ok(json!(res))
                        }
                    }
                } else {
                    if data.get("error").is_some() {
                        Err(json!(data["error"]["message"]))
                    } else {
                        Ok(json!("ok"))
                    }
                }
            }
            Err(err) => Err(json!(err.to_string())),
        },
        Err(err) => Err(json!(err.to_string())),
    }
}
