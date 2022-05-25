use std::sync::Arc;

use crate::p2p::behaviour::{
    group::GroupId,
    rpc::{Request, RequestId, Response},
};
use crate::p2p::rpc::helper::{json, RpcError, RpcHandler, RpcParam};
use crate::p2p::server::Event;

pub struct State;

pub fn init_rpc_handler() -> RpcHandler<State> {
    let mut rpc_handler = RpcHandler::new(State {});

    rpc_handler.add_method(
        "echo",
        |params: Vec<RpcParam>, _state: Arc<State>| async move {
            Ok(vec![Event::Rpc(json!(params))])
        },
    );

    rpc_handler.add_method(
        "connect",
        |params: Vec<RpcParam>, _state: Arc<State>| async move {
            if params.len() != 1 {
                return Err(RpcError::ParseError);
            }
            let s = params[0].as_str().ok_or(RpcError::ParseError)?;
            let addr = s.parse().map_err(|_e| RpcError::InvalidRequest)?;

            Ok(vec![Event::Connect(addr), Event::Rpc(Default::default())])
        },
    );

    rpc_handler.add_method(
        "query",
        |params: Vec<RpcParam>, _state: Arc<State>| async move {
            if params.len() != 3 && params.len() != 4 {
                return Err(RpcError::ParseError);
            }
            let s = params[0].as_str().ok_or(RpcError::ParseError)?;
            let pid = s.parse().map_err(|_e| RpcError::InvalidRequest)?;
            let project = params[1].as_str().ok_or(RpcError::ParseError)?.to_owned();
            let query = params[2].as_str().ok_or(RpcError::ParseError)?.to_owned();
            let sign = if params.len() == 4 {
                params[3].as_str().ok_or(RpcError::ParseError)?.to_owned()
            } else {
                "".to_owned()
            };

            Ok(vec![Event::Request(
                pid,
                Request::Query(project, query, sign),
            )])
        },
    );

    rpc_handler.add_method(
        "state-channel",
        |params: Vec<RpcParam>, _state: Arc<State>| async move {
            if params.len() != 2 {
                return Err(RpcError::ParseError);
            }
            let s = params[0].as_str().ok_or(RpcError::ParseError)?;
            let pid = s.parse().map_err(|_e| RpcError::InvalidRequest)?;
            let infos = params[1].as_str().ok_or(RpcError::ParseError)?;

            Ok(vec![Event::RequestSync(
                pid,
                Request::StateChannel(infos.to_owned()),
            )])
        },
    );

    rpc_handler.add_method(
        "query-sync",
        |params: Vec<RpcParam>, _state: Arc<State>| async move {
            info!("params: {:?}", params);
            if params.len() != 3 && params.len() != 4 {
                return Err(RpcError::ParseError);
            }
            let s = params[0].as_str().ok_or(RpcError::ParseError)?;
            let pid = s.parse().map_err(|_e| RpcError::InvalidRequest)?;
            let project = params[1].as_str().ok_or(RpcError::ParseError)?.to_owned();
            let query = params[2].as_str().ok_or(RpcError::ParseError)?.to_owned();
            let sign = if params.len() == 4 {
                params[3].as_str().ok_or(RpcError::ParseError)?.to_owned()
            } else {
                "".to_owned()
            };

            Ok(vec![Event::RequestSync(
                pid,
                Request::Query(project, query, sign),
            )])
        },
    );

    rpc_handler.add_method(
        "response",
        |params: Vec<RpcParam>, _state: Arc<State>| async move {
            if params.len() != 2 {
                return Err(RpcError::ParseError);
            }
            let uid = params[0].as_i64().ok_or(RpcError::ParseError)? as RequestId;
            let msg = params[1].as_str().ok_or(RpcError::ParseError)?;

            Ok(vec![
                Event::Response(uid, Response::RawData(msg.to_owned())),
                Event::Rpc(Default::default()),
            ])
        },
    );

    rpc_handler.add_method(
        "group-join",
        |params: Vec<RpcParam>, _state: Arc<State>| async move {
            if params.len() != 1 {
                return Err(RpcError::ParseError);
            }
            let gid = params[0].as_str().ok_or(RpcError::ParseError)?;

            Ok(vec![
                Event::GroupJoin(GroupId::new(gid)),
                Event::Rpc(Default::default()),
            ])
        },
    );

    rpc_handler.add_method(
        "group-leave",
        |params: Vec<RpcParam>, _state: Arc<State>| async move {
            if params.len() != 1 {
                return Err(RpcError::ParseError);
            }
            let gid = params[0].as_str().ok_or(RpcError::ParseError)?;

            Ok(vec![
                Event::GroupLeave(GroupId::new(gid)),
                Event::Rpc(Default::default()),
            ])
        },
    );

    rpc_handler.add_method(
        "group-broadcast",
        |params: Vec<RpcParam>, _state: Arc<State>| async move {
            if params.len() != 2 {
                return Err(RpcError::ParseError);
            }
            let gid = params[0].as_str().ok_or(RpcError::ParseError)?;
            let msg = params[1].as_str().ok_or(RpcError::ParseError)?;

            Ok(vec![
                Event::GroupBroadcast(GroupId::new(gid), msg.as_bytes().to_vec()),
                Event::Rpc(Default::default()),
            ])
        },
    );

    rpc_handler.add_method(
        "group-add-node",
        |params: Vec<RpcParam>, _state: Arc<State>| async move {
            if params.len() != 2 {
                return Err(RpcError::ParseError);
            }
            let gid = params[0].as_str().ok_or(RpcError::ParseError)?;
            let s = params[1].as_str().ok_or(RpcError::ParseError)?;
            let pid = s.parse().map_err(|_e| RpcError::InvalidRequest)?;

            Ok(vec![
                Event::GroupAddNode(GroupId::new(gid), pid),
                Event::Rpc(Default::default()),
            ])
        },
    );

    rpc_handler.add_method(
        "group-del-node",
        |params: Vec<RpcParam>, _state: Arc<State>| async move {
            if params.len() != 2 {
                return Err(RpcError::ParseError);
            }
            let gid = params[0].as_str().ok_or(RpcError::ParseError)?;
            let s = params[1].as_str().ok_or(RpcError::ParseError)?;
            let pid = s.parse().map_err(|_e| RpcError::InvalidRequest)?;

            Ok(vec![
                Event::GroupDelNode(GroupId::new(gid), pid),
                Event::Rpc(Default::default()),
            ])
        },
    );

    rpc_handler
}
