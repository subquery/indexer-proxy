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

use futures::StreamExt;
use libp2p::{
    core::either::EitherError,
    identity::Keypair,
    ping::Failure,
    swarm::{handler::ConnectionHandlerUpgrErr, Swarm, SwarmBuilder, SwarmEvent},
    Multiaddr, PeerId,
};
use std::{collections::HashMap, error::Error, net::SocketAddr};
use tokio::{
    select,
    sync::mpsc::{Receiver, Sender},
};

use super::behaviour::{
    behaviour,
    group::{GroupEvent, GroupId, GroupMessage},
    rpc::{Request, RequestId, Response, RpcEvent, RpcMessage as NetworkRpcMessage},
    Behaviour, Event as NetworkEvent,
};
use super::handler::init_rpc_handler;
use super::rpc::{
    helper::{rpc_error, rpc_response, RpcParam},
    rpc_channel, start as rpc_start, RpcConfig, RpcMessage,
};
use super::P2pHandler;

pub async fn server<T: P2pHandler>(
    p2p_addr: Multiaddr,
    rpc_addr: SocketAddr,
    ws_addr: Option<SocketAddr>,
    _channel: Option<(Sender<ChannelMessage>, Receiver<ChannelMessage>)>,
    key: Keypair,
) -> Result<Swarm<Behaviour>, Box<dyn Error>> {
    let peer_id = PeerId::from(key.public());
    info!("Local peer id: {:?}", peer_id);

    let transport = libp2p::tokio_development_transport(key)?;
    let mut swarm = SwarmBuilder::new(transport, behaviour(peer_id), peer_id)
        .executor(Box::new(|fut| {
            tokio::spawn(fut);
        }))
        .build();

    swarm.listen_on(p2p_addr)?;

    // DEBUG auto join subquery
    swarm.behaviour_mut().group.join(GroupId::new("subquery"));

    let (out_send, mut out_recv) = rpc_channel();
    let rpc_config = RpcConfig {
        addr: rpc_addr,
        ws: ws_addr,
        index: None,
    };
    let rpc_send = rpc_start(rpc_config, out_send).await.unwrap();
    let rpc_handler = init_rpc_handler();

    // store the sync requests. request_id => (rpc_id, is_ws)
    let mut sync_requests: HashMap<RequestId, (u64, bool)> = HashMap::new();

    loop {
        let res = select! {
            v = async { out_recv.recv().await.map(|rpc| FutureResult::Rpc(rpc)) } => v.unwrap(),
            v = async {
                let event = swarm.select_next_some().await;
                FutureResult::P2p(event)
            } => v
        };

        match res {
            FutureResult::P2p(event) => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    debug!("P2P Listening on {:?}", address);
                }
                SwarmEvent::Behaviour(event) => match event {
                    NetworkEvent::Rpc(msg) => match msg {
                        RpcEvent::Message { peer: _, message } => match message {
                            NetworkRpcMessage::Request { request_id, request } => {
                                debug!("Got request: {:?}", request);
                                let res = T::request(request).await;
                                let _ = swarm.behaviour_mut().rpc.response(request_id, res);
                            }
                            NetworkRpcMessage::Response { request_id, response } => {
                                debug!("Got response: {:?}", response);
                                let res = match response {
                                    Response::Data(data) => rpc_response(0, "data", RpcParam::from(data)),
                                    Response::Error(msg) => rpc_error(0, &msg),
                                    Response::StateChannel(infos) => {
                                        rpc_response(0, "state-channel", RpcParam::from(infos))
                                    }
                                };

                                if let Some((uid, is_ws)) = sync_requests.remove(&request_id) {
                                    let _ = rpc_send.send(RpcMessage(uid, res, is_ws)).await;
                                } else {
                                    // send to all connected ws.
                                    let _ = rpc_send.send(RpcMessage(0, res, true)).await;
                                }
                            }
                        },
                        RpcEvent::OutboundFailure {
                            peer: _,
                            request_id: _,
                            error: _,
                        } => {
                            // handle send request/response error.
                        }
                        RpcEvent::InboundFailure {
                            peer: _,
                            request_id: _,
                            error: _,
                        } => {
                            // handle receive request/response error.
                        }
                        RpcEvent::ResponseSent { peer: _, request_id: _ } => {
                            // handle send response success.
                        }
                    },
                    NetworkEvent::Group(msg) => {
                        match msg {
                            GroupEvent::Message(GroupMessage {
                                source,
                                group,
                                sequence: _,
                                data,
                            }) => {
                                // handle received data
                                let s = String::from_utf8(data).unwrap_or(Default::default());
                                debug!("Group: {} Message from {}: {:?}", group, source, s);
                            }
                            GroupEvent::Join { peer: _, group: _ } => {
                                // handle peer join.
                            }
                            GroupEvent::Leave { peer: _, group: _ } => {
                                // handle per leave.
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            },
            FutureResult::Rpc(RpcMessage(uid, params, is_ws)) => {
                if let Ok(mut events) = rpc_handler.handle(params).await {
                    loop {
                        if events.len() != 0 {
                            match events.remove(0) {
                                Event::Rpc(msg) => {
                                    let _ = rpc_send.send(RpcMessage(uid, msg, is_ws)).await;
                                }
                                Event::Connect(addr) => {
                                    let _ = swarm.dial(addr);
                                }
                                Event::Request(pid, req) => {
                                    let req_id = swarm.behaviour_mut().rpc.request(pid, req);
                                    let res = rpc_response(0, "request", RpcParam::from(req_id));
                                    let _ = rpc_send.send(RpcMessage(uid, res, is_ws)).await;
                                }
                                Event::RequestSync(pid, req) => {
                                    let req_id = swarm.behaviour_mut().rpc.request(pid, req);
                                    sync_requests.insert(req_id, (uid, is_ws));
                                }
                                Event::Response(rid, res) => {
                                    let _ = swarm.behaviour_mut().rpc.response(rid, res);
                                }
                                Event::GroupJoin(gid) => {
                                    let _ = swarm.behaviour_mut().group.join(gid);
                                }
                                Event::GroupLeave(gid) => {
                                    let _ = swarm.behaviour_mut().group.leave(gid);
                                }
                                Event::GroupBroadcast(gid, data) => {
                                    let _ = swarm.behaviour_mut().group.broadcast(gid, data);
                                }
                                Event::GroupAddNode(gid, pid) => {
                                    let _ = swarm.behaviour_mut().group.add_node_to_group(gid, pid);
                                }
                                Event::GroupDelNode(gid, pid) => {
                                    let _ = swarm.behaviour_mut().group.remove_node_from_group(gid, pid);
                                }
                            }
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }
}

enum FutureResult {
    Rpc(RpcMessage),
    P2p(
        SwarmEvent<
            NetworkEvent,
            EitherError<
                EitherError<Failure, ConnectionHandlerUpgrErr<std::io::Error>>,
                ConnectionHandlerUpgrErr<std::io::Error>,
            >,
        >,
    ),
}

pub enum Event {
    Rpc(RpcParam),
    Connect(Multiaddr),
    Request(PeerId, Request),
    RequestSync(PeerId, Request),
    Response(RequestId, Response),
    GroupJoin(GroupId),
    GroupLeave(GroupId),
    GroupBroadcast(GroupId, Vec<u8>),
    GroupAddNode(GroupId, PeerId),
    GroupDelNode(GroupId, PeerId),
}

pub struct ChannelMessage(u64, Event);
