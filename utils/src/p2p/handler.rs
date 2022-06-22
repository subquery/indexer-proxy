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

use std::sync::Arc;

use super::behaviour::{
    group::GroupId,
    rpc::{Request, RequestId, Response},
};
use super::rpc::helper::{json, RpcError, RpcHandler, RpcParam};
use super::server::Event;

pub struct State;

pub fn init_rpc_handler() -> RpcHandler<State> {
    let mut rpc_handler = RpcHandler::new(State {});

    rpc_handler.add_method("echo", |params: Vec<RpcParam>, _state: Arc<State>| async move {
        Ok(vec![Event::Rpc(json!(params))])
    });

    rpc_handler.add_method("connect", |params: Vec<RpcParam>, _state: Arc<State>| async move {
        if params.len() != 1 {
            return Err(RpcError::ParseError);
        }
        let s = params[0].as_str().ok_or(RpcError::ParseError)?;
        let addr = s.parse().map_err(|_e| RpcError::InvalidRequest)?;

        Ok(vec![Event::Connect(addr), Event::Rpc(Default::default())])
    });

    rpc_handler.add_method(
        "state-channel",
        |params: Vec<RpcParam>, _state: Arc<State>| async move {
            if params.len() != 2 {
                return Err(RpcError::ParseError);
            }
            let s = params[0].as_str().ok_or(RpcError::ParseError)?;
            let pid = s.parse().map_err(|_e| RpcError::InvalidRequest)?;
            let sign = params[1].as_str().ok_or(RpcError::ParseError)?;
            let query = serde_json::to_string(&json!({
                "method": "open",
                "state": sign,
            }))
            .unwrap();

            Ok(vec![Event::RequestSync(pid, Request::StateChannel(query))])
        },
    );

    rpc_handler.add_method("payg", |params: Vec<RpcParam>, _state: Arc<State>| async move {
        if params.len() != 4 {
            return Err(RpcError::ParseError);
        }
        let s = params[0].as_str().ok_or(RpcError::ParseError)?;
        let pid = s.parse().map_err(|_e| RpcError::InvalidRequest)?;
        let project = params[1].as_str().ok_or(RpcError::ParseError)?.to_owned();
        let query = params[2].as_str().ok_or(RpcError::ParseError)?.to_owned();
        let sign = params[3].as_str().ok_or(RpcError::ParseError)?.to_owned();
        let query = serde_json::to_string(&json!({
            "method": "query",
            "project": project,
            "query": query,
            "state": sign,
        }))
        .unwrap();

        Ok(vec![Event::Request(pid, Request::StateChannel(query))])
    });

    rpc_handler.add_method("payg-sync", |params: Vec<RpcParam>, _state: Arc<State>| async move {
        if params.len() != 4 {
            return Err(RpcError::ParseError);
        }
        let s = params[0].as_str().ok_or(RpcError::ParseError)?;
        let pid = s.parse().map_err(|_e| RpcError::InvalidRequest)?;
        let project = params[1].as_str().ok_or(RpcError::ParseError)?.to_owned();
        let query = params[2].as_str().ok_or(RpcError::ParseError)?.to_owned();
        let sign = params[3].as_str().ok_or(RpcError::ParseError)?.to_owned();
        let query = serde_json::to_string(&json!({
            "method": "query",
            "project": project,
            "query": query,
            "state": sign,
        }))
        .unwrap();

        Ok(vec![Event::RequestSync(pid, Request::StateChannel(query))])
    });

    rpc_handler.add_method("response", |params: Vec<RpcParam>, _state: Arc<State>| async move {
        if params.len() != 2 {
            return Err(RpcError::ParseError);
        }
        let uid = params[0].as_i64().ok_or(RpcError::ParseError)? as RequestId;
        let msg = params[1].as_str().ok_or(RpcError::ParseError)?;

        Ok(vec![
            Event::Response(uid, Response::Data(msg.to_owned())),
            Event::Rpc(Default::default()),
        ])
    });

    rpc_handler.add_method("group-join", |params: Vec<RpcParam>, _state: Arc<State>| async move {
        if params.len() != 1 {
            return Err(RpcError::ParseError);
        }
        let gid = params[0].as_str().ok_or(RpcError::ParseError)?;

        Ok(vec![
            Event::GroupJoin(GroupId::new(gid)),
            Event::Rpc(Default::default()),
        ])
    });

    rpc_handler.add_method("group-leave", |params: Vec<RpcParam>, _state: Arc<State>| async move {
        if params.len() != 1 {
            return Err(RpcError::ParseError);
        }
        let gid = params[0].as_str().ok_or(RpcError::ParseError)?;

        Ok(vec![
            Event::GroupLeave(GroupId::new(gid)),
            Event::Rpc(Default::default()),
        ])
    });

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
