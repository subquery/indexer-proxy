use serde_json::{json, Value};
use warp::http::header::AUTHORIZATION;

use crate::p2p::behaviour::rpc::Response;
use crate::project::get_project;
use crate::request::{graphql_request, REQUEST_CLIENT};

pub async fn query_request(project: String, query: String) -> Response {
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

pub async fn proxy_request(
    method: &str,
    url: &str,
    path: &str,
    token: &str,
    query: String,
    headers: Vec<(String, String)>,
) -> Result<Value, Value> {
    let url = format!("{}/{}", url, path);
    let token = format!("Bearer {}", token);

    let res = match method.to_lowercase().as_str() {
        "get" => {
            let mut req = REQUEST_CLIENT.get(url).header(AUTHORIZATION, token);
            for (k, v) in headers {
                req = req.header(k, v);
            }
            req.send().await
        }
        _ => {
            let mut req = REQUEST_CLIENT
                .post(url)
                .header("content-type", "application/json")
                .header(AUTHORIZATION, token);
            for (k, v) in headers {
                req = req.header(k, v);
            }
            req.body(query).send().await
        }
    };

    match res {
        Ok(res) => match res.error_for_status() {
            Ok(res) => match res.text().await {
                Ok(data) => match serde_json::from_str(&data) {
                    Ok(data) => Ok(data),
                    Err(_err) => Ok(json!(data)),
                },
                Err(err) => Err(json!(err.to_string())),
            },
            Err(err) => Err(json!(err.to_string())),
        },
        Err(err) => Err(json!(err.to_string())),
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
