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

use futures::{SinkExt, StreamExt};
use rand_chacha::{
    rand_core::{RngCore, SeedableRng},
    ChaChaRng,
};
use std::io::{Error, ErrorKind, Result};
use std::net::SocketAddr;
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    sync::mpsc::Sender,
};
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message as WsMessage};

use super::helper::parse_jsonrpc;
use super::{rpc_inner_channel, RpcInnerMessage};

pub(super) async fn ws_listen(send: Sender<RpcInnerMessage>, listener: TcpListener) -> Result<()> {
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(ws_connection(send.clone(), stream, addr));
    }

    Ok(())
}

enum FutureResult {
    Out(RpcInnerMessage),
    Stream(WsMessage),
}

async fn ws_connection(send: Sender<RpcInnerMessage>, raw_stream: TcpStream, addr: SocketAddr) -> Result<()> {
    let ws_stream = accept_async(raw_stream)
        .await
        .map_err(|_e| Error::new(ErrorKind::Other, "Accept WebSocket Failure!"))?;
    debug!("DEBUG: WebSocket connection established: {}", addr);

    let mut rng = ChaChaRng::from_entropy();
    let id: u64 = rng.next_u64();
    let (s_send, mut s_recv) = rpc_inner_channel();
    send.send(RpcInnerMessage::Open(id, s_send))
        .await
        .expect("Ws to Rpc channel closed");

    let (mut writer, mut reader) = ws_stream.split();

    loop {
        let res = select! {
            v = async { s_recv.recv().await.map(|msg| FutureResult::Out(msg)) } => v,
            v = async {
                reader
                    .next()
                    .await
                    .map(|msg| msg.map(|msg| FutureResult::Stream(msg)).ok())
                    .flatten()
            } => v,
        };

        match res {
            Some(FutureResult::Out(msg)) => {
                let param = match msg {
                    RpcInnerMessage::Response(param) => param,
                    _ => Default::default(),
                };
                let s = WsMessage::from(param.to_string());
                let _ = writer.send(s).await;
            }
            Some(FutureResult::Stream(msg)) => {
                let msg = msg.to_text().unwrap();
                match parse_jsonrpc(msg.to_owned()) {
                    Ok(rpc_param) => {
                        send.send(RpcInnerMessage::Request(id, rpc_param, None))
                            .await
                            .expect("Ws to Rpc channel closed");
                    }
                    Err((err, id)) => {
                        let s = WsMessage::from(err.json(id).to_string());
                        let _ = writer.send(s).await;
                    }
                }
            }
            None => break,
        }
    }

    send.send(RpcInnerMessage::Close(id))
        .await
        .expect("Ws to Rpc channel closed");
    Ok(())
}
