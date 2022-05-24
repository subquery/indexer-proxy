use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::sync::Arc;

pub use serde_json::json;
pub type RpcParam = Value;

use crate::p2p::server::Event;

#[derive(Debug, Clone)]
pub enum RpcError {
    ParseError,
    InvalidRequest,
    InvalidVersion,
    InvalidResponse,
    MethodNotFound(String),
    Custom(String),
}

impl Into<std::io::Error> for RpcError {
    fn into(self) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::Other, "RPC Error")
    }
}

impl From<std::io::Error> for RpcError {
    fn from(e: std::io::Error) -> RpcError {
        RpcError::Custom(format!("{}", e))
    }
}

impl From<bincode::Error> for RpcError {
    fn from(e: bincode::Error) -> RpcError {
        RpcError::Custom(format!("{}", e))
    }
}

impl RpcError {
    pub fn json(&self, id: u64) -> RpcParam {
        match self {
            RpcError::ParseError => json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32700,
                    "message": "Parse error"
                }
            }),
            RpcError::MethodNotFound(method) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Method {} not found", method)
                }
            }),
            RpcError::InvalidRequest => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32600,
                    "message": "Invalid Request"
                }
            }),
            RpcError::InvalidVersion => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32600,
                    "message": "Unsupported JSON-RPC protocol version"
                }
            }),
            RpcError::InvalidResponse => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32600,
                    "message": "Invalid Response"
                }
            }),
            RpcError::Custom(m) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32600,
                    "message": m
                }
            }),
        }
    }
}

pub fn parse_jsonrpc(json_string: String) -> std::result::Result<RpcParam, (RpcError, u64)> {
    match serde_json::from_str::<RpcParam>(&json_string) {
        Ok(mut value) => {
            let id_res = value
                .get("id")
                .map(|id| {
                    id.as_u64()
                        .or(id.as_str().map(|sid| sid.parse::<u64>().ok()).flatten())
                })
                .flatten();

            if id_res.is_none() {
                return Err((RpcError::ParseError, 0));
            }
            let id = id_res.unwrap();
            *value.get_mut("id").unwrap() = id.into();

            // check if json is response
            if value.get("result").is_some() || value.get("error").is_some() {
                return Err((RpcError::InvalidResponse, id));
            }

            if value.get("method").is_none() || value.get("method").unwrap().as_str().is_none() {
                return Err((RpcError::InvalidRequest, id));
            }

            if value.get("params").is_none() {
                value["params"] = RpcParam::Array(vec![]);
            }

            let jsonrpc = value
                .get("jsonrpc")
                .map(|v| {
                    v.as_str()
                        .map(|s| if s == "2.0" { Some(2) } else { None })
                        .flatten()
                })
                .flatten();

            if jsonrpc.is_none() {
                return Err((RpcError::InvalidVersion, id));
            }

            Ok(value)
        }
        Err(_e) => Err((RpcError::ParseError, 0)),
    }
}

pub struct RpcHandler<S: Send + Sync> {
    state: Arc<S>,
    fns: HashMap<&'static str, Box<DynFutFn<S>>>,
}

type RpcResult = std::result::Result<Vec<Event>, RpcError>;
type BoxFuture<RpcResult> = Pin<Box<dyn Future<Output = RpcResult> + Send>>;

pub trait FutFn<S>: Send + Sync + 'static {
    fn call(&self, params: Vec<RpcParam>, s: Arc<S>) -> BoxFuture<RpcResult>;
}

pub(crate) type DynFutFn<S> = dyn FutFn<S>;

impl<S, F: Send + Sync + 'static, Fut> FutFn<S> for F
where
    F: Fn(Vec<RpcParam>, Arc<S>) -> Fut,
    Fut: Future<Output = RpcResult> + Send + 'static,
{
    fn call(&self, params: Vec<RpcParam>, s: Arc<S>) -> BoxFuture<RpcResult> {
        let fut = (self)(params, s);
        Box::pin(async move { fut.await })
    }
}

impl<S: 'static + Send + Sync> RpcHandler<S> {
    pub fn new(state: S) -> RpcHandler<S> {
        Self {
            state: Arc::new(state),
            fns: HashMap::new(),
        }
    }

    pub fn add_method(&mut self, name: &'static str, f: impl FutFn<S>) {
        self.fns.insert(name, Box::new(f));
    }

    pub async fn handle(&self, mut param: RpcParam) -> Result<Vec<Event>> {
        let id = param["id"].take().as_u64().unwrap();
        let method_s = param["method"].take();
        let method = method_s.as_str().unwrap();
        let mut new_results = vec![];

        if method == "rpcs" {
            let mut methods: Vec<&str> = self.fns.keys().map(|v| *v).collect();
            methods.sort();
            let params = json!(methods);

            new_results.push(Event::Rpc(rpc_response(id, method, params)));

            return Ok(new_results);
        }

        if let RpcParam::Array(params) = param["params"].take() {
            match self.fns.get(method) {
                Some(f) => {
                    let res = f.call(params, self.state.clone()).await;
                    match res {
                        Ok(events) => {
                            for event in events {
                                match event {
                                    Event::Rpc(params) => {
                                        // check when params is complete jsonrpc result.
                                        if params.is_object() && params.get("jsonrpc").is_some() {
                                            new_results.push(Event::Rpc(params));
                                            continue;
                                        }
                                        new_results
                                            .push(Event::Rpc(rpc_response(id, method, params)));
                                    }
                                    _ => new_results.push(event),
                                }
                            }
                        }
                        Err(err) => {
                            let mut res = err.json(id);
                            res["method"] = method.into();
                            new_results.push(Event::Rpc(res));
                        }
                    }
                }
                None => new_results.push(Event::Rpc(
                    RpcError::MethodNotFound(method.to_owned()).json(id),
                )),
            }
        } else {
            new_results.push(Event::Rpc(RpcError::InvalidRequest.json(id)))
        }

        Ok(new_results)
    }
}

pub fn rpc_response(id: u64, method: &str, params: RpcParam) -> RpcParam {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "result": params
    })
}

pub fn rpc_error(id: u64, msg: &str) -> RpcParam {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": 400,
            "message": msg
        }
    })
}
