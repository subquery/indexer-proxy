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

use rand_chacha::{
    rand_core::{RngCore, SeedableRng},
    ChaChaRng,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt, Result},
    net::{TcpListener, TcpStream},
    sync::mpsc::Sender,
    sync::RwLock,
};

use super::helper::parse_jsonrpc;
use super::{rpc_inner_channel, RpcInnerMessage};

pub(super) async fn http_listen(
    index: Option<PathBuf>,
    send: Sender<RpcInnerMessage>,
    listener: TcpListener,
) -> Result<()> {
    let homepage = if let Some(path) = index {
        fs::read_to_string(path).await.unwrap_or("Error Homepage.".to_owned())
    } else {
        "No Homepage.".to_owned()
    };
    let homelink = Arc::new(RwLock::new(homepage));

    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(http_connection(homelink.clone(), send.clone(), stream, addr));
    }

    Ok(())
}

enum HTTP {
    Ok(usize),
    NeedMore(usize, usize),
}

fn parse_req<'a>(src: &[u8]) -> std::result::Result<HTTP, &'a str> {
    let mut req_parsed_headers = [httparse::EMPTY_HEADER; 16];
    let mut req = httparse::Request::new(&mut req_parsed_headers);
    let status = req.parse(&src).map_err(|_| "HTTP parse error")?;

    let content_length_headers: Vec<httparse::Header> = req
        .headers
        .iter()
        .filter(|header| header.name.to_ascii_lowercase() == "content-length")
        .cloned()
        .collect();

    if content_length_headers.len() != 1 {
        return Err("HTTP header is invalid");
    }

    let length_bytes = content_length_headers.first().unwrap().value;
    let mut length_string = String::new();

    for b in length_bytes {
        length_string.push(*b as char);
    }

    let length = length_string.parse::<usize>().map_err(|_| "HTTP length is invalid")?;

    let amt = match status {
        httparse::Status::Complete(amt) => amt,
        httparse::Status::Partial => return Err("HTTP parse error"),
    };

    if src[amt..].len() >= length {
        return Ok(HTTP::Ok(amt));
    }

    Ok(HTTP::NeedMore(amt, length))
}

async fn http_connection(
    _homelink: Arc<RwLock<String>>,
    send: Sender<RpcInnerMessage>,
    mut stream: TcpStream,
    addr: SocketAddr,
) -> Result<()> {
    debug!("DEBUG: HTTP connection established: {}", addr);
    let mut rng = ChaChaRng::from_entropy();
    let id: u64 = rng.next_u64();
    let (s_send, mut s_recv) = rpc_inner_channel();

    let mut buf = vec![];

    // TODO add timeout
    let mut tmp_buf = vec![0u8; 1024];
    let n = stream.read(&mut tmp_buf).await?;
    let body = match parse_req(&tmp_buf[..n]) {
        Ok(HTTP::NeedMore(amt, len)) => {
            buf.extend(&tmp_buf[amt..n]);
            loop {
                let mut tmp = vec![0u8; 1024];
                let n = stream.read(&mut tmp).await?;
                buf.extend(&tmp[..n]);
                if buf.len() >= len {
                    break;
                }
            }
            &buf[..]
        }
        Ok(HTTP::Ok(amt)) => &tmp_buf[amt..n],
        Err(e) => {
            info!("TDN: HTTP JSONRPC parse error: {}", e);
            return Ok(());
        }
    };

    let msg = String::from_utf8_lossy(body);
    let res =
        "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin:*;\r\nContent-Type:application/json;charset=UTF-8\r\n\r\n";

    match parse_jsonrpc((*msg).to_string()) {
        Ok(rpc_param) => {
            send.send(RpcInnerMessage::Request(id, rpc_param, Some(s_send)))
                .await
                .expect("Http to Rpc channel closed");
        }
        Err((err, id)) => {
            stream
                .write(format!("{}{}", res, err.json(id).to_string()).as_bytes())
                .await?;
            let _ = stream.flush().await;
            stream.shutdown().await?;
        }
    }

    while let Some(msg) = s_recv.recv().await {
        let param = match msg {
            RpcInnerMessage::Response(param) => param,
            _ => Default::default(),
        };
        stream.write(format!("{}{}", res, param.to_string()).as_bytes()).await?;
        let _ = stream.flush().await;
        stream.shutdown().await?;
        break;
    }

    Ok(())
}
