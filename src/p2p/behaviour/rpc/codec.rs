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
        bincode::deserialize(&bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "RPC request deserialize error"))
    }

    /// Reads a response from the given I/O stream according to the
    /// negotiated protocol.
    pub async fn read_response<T>(_protocol: &SubqueryProtocol, io: &mut T) -> io::Result<Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let bytes = upgrade::read_length_prefixed(io, MAX_NETWORK_DATA_LEN).await?;
        bincode::deserialize(&bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "RPC response deserialize error"))
    }

    /// Writes a request to the given I/O stream according to the
    /// negotiated protocol.
    pub async fn write_request<T>(
        _protocol: &SubqueryProtocol,
        io: &mut T,
        req: Request,
    ) -> io::Result<()>
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
    pub async fn write_response<T>(
        _protocol: &SubqueryProtocol,
        io: &mut T,
        res: Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let bytes = bincode::serialize(&res)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "RPC response serialize error"))?;
        upgrade::write_length_prefixed(io, bytes).await?;
        io.close().await
    }
}
