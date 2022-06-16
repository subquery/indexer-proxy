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

use std::collections::HashMap;
use std::io::Result;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::{
    net::TcpListener,
    select,
    sync::mpsc::{self, Receiver, Sender},
};

pub mod helper;
mod http;
mod ws;

use helper::RpcParam;

pub struct RpcConfig {
    pub addr: SocketAddr,
    pub ws: Option<SocketAddr>,
    pub index: Option<PathBuf>,
}

/// packaging the rpc message. not open to ouside.
#[derive(Debug)]
pub struct RpcMessage(pub u64, pub RpcParam, pub bool);

pub fn rpc_channel() -> (Sender<RpcMessage>, Receiver<RpcMessage>) {
    mpsc::channel(128)
}

pub async fn start(config: RpcConfig, send: Sender<RpcMessage>) -> Result<Sender<RpcMessage>> {
    let (out_send, out_recv) = rpc_channel();

    let (self_send, self_recv) = rpc_inner_channel();

    server(self_send, config).await?;
    listen(send, out_recv, self_recv).await?;

    Ok(out_send)
}

#[derive(Debug)]
enum RpcInnerMessage {
    Open(u64, Sender<RpcInnerMessage>),
    Close(u64),
    Request(u64, RpcParam, Option<Sender<RpcInnerMessage>>),
    Response(RpcParam),
}

fn rpc_inner_channel() -> (Sender<RpcInnerMessage>, Receiver<RpcInnerMessage>) {
    mpsc::channel(128)
}

enum FutureResult {
    Out(RpcMessage),
    Stream(RpcInnerMessage),
}

async fn listen(
    send: Sender<RpcMessage>,
    mut out_recv: Receiver<RpcMessage>,
    mut self_recv: Receiver<RpcInnerMessage>,
) -> Result<()> {
    tokio::spawn(async move {
        let mut connections: HashMap<u64, (Sender<RpcInnerMessage>, bool)> = HashMap::new();

        loop {
            let res = select! {
                v = async { out_recv.recv().await.map(|msg| FutureResult::Out(msg)) } => v,
                v = async { self_recv.recv().await.map(|msg| FutureResult::Stream(msg)) } => v
            };

            match res {
                Some(FutureResult::Out(msg)) => {
                    let RpcMessage(id, params, is_ws) = msg;
                    if is_ws {
                        if id == 0 {
                            // default send to all ws.
                            for (_, (s, iw)) in &connections {
                                if *iw {
                                    let _ = s.send(RpcInnerMessage::Response(params.clone())).await;
                                }
                            }
                        } else {
                            if let Some((s, _)) = connections.get(&id) {
                                let _ = s.send(RpcInnerMessage::Response(params)).await;
                            }
                        }
                    } else {
                        let s = connections.remove(&id);
                        if s.is_some() {
                            let _ = s.unwrap().0.send(RpcInnerMessage::Response(params)).await;
                        }
                    }
                }
                Some(FutureResult::Stream(msg)) => {
                    match msg {
                        RpcInnerMessage::Request(uid, params, sender) => {
                            let is_ws = sender.is_none();
                            if !is_ws {
                                connections.insert(uid, (sender.unwrap(), false));
                            }
                            send.send(RpcMessage(uid, params, is_ws))
                                .await
                                .expect("Rpc to Outside channel closed");
                        }
                        RpcInnerMessage::Open(id, sender) => {
                            connections.insert(id, (sender, true));
                        }
                        RpcInnerMessage::Close(id) => {
                            connections.remove(&id);
                        }
                        _ => {} // others not handle
                    }
                }
                None => break,
            }
        }
    });

    Ok(())
}

async fn server(send: Sender<RpcInnerMessage>, config: RpcConfig) -> Result<()> {
    tokio::spawn(http::http_listen(
        config.index.clone(),
        send.clone(),
        TcpListener::bind(config.addr).await.map_err(|e| {
            error!("RPC HTTP listen {:?}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "TCP Listen")
        })?,
    ));

    // ws
    if config.ws.is_some() {
        tokio::spawn(ws::ws_listen(
            send,
            TcpListener::bind(config.ws.unwrap()).await.map_err(|e| {
                error!("RPC WS listen {:?}", e);
                std::io::Error::new(std::io::ErrorKind::Other, "TCP Listen")
            })?,
        ));
    }

    Ok(())
}
