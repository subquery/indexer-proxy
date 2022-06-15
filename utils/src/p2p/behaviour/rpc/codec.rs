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

use futures::prelude::*;
use libp2p::core::upgrade;
use std::io;

use super::{Request, Response};
use crate::p2p::primitives::{SubqueryProtocol, MAX_NETWORK_DATA_LEN};

pub struct RpcCodec;

impl RpcCodec {
    /// Reads a request from the given I/O stream according to the
    /// negotiated protocol.
    pub async fn read_request<T>(_protocol: &SubqueryProtocol, io: &mut T) -> io::Result<Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let bytes = upgrade::read_length_prefixed(io, MAX_NETWORK_DATA_LEN).await?;
        bincode::deserialize(&bytes).map_err(|_| io::Error::new(io::ErrorKind::Other, "RPC request deserialize error"))
    }

    /// Reads a response from the given I/O stream according to the
    /// negotiated protocol.
    pub async fn read_response<T>(_protocol: &SubqueryProtocol, io: &mut T) -> io::Result<Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let bytes = upgrade::read_length_prefixed(io, MAX_NETWORK_DATA_LEN).await?;
        bincode::deserialize(&bytes).map_err(|_| io::Error::new(io::ErrorKind::Other, "RPC response deserialize error"))
    }

    /// Writes a request to the given I/O stream according to the
    /// negotiated protocol.
    pub async fn write_request<T>(_protocol: &SubqueryProtocol, io: &mut T, req: Request) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let bytes = bincode::serialize(&req)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "RPC request serialize error"))?;
        upgrade::write_length_prefixed(io, bytes).await?;
        io.close().await
    }

    /// Writes a response to the given I/O stream according to the
    /// negotiated protocol.
    pub async fn write_response<T>(_protocol: &SubqueryProtocol, io: &mut T, res: Response) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let bytes = bincode::serialize(&res)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "RPC response serialize error"))?;
        upgrade::write_length_prefixed(io, bytes).await?;
        io.close().await
    }
}
