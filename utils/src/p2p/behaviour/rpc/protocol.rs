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

//! The definition of a request/response protocol via inbound
//! and outbound substream upgrades. The inbound upgrade
//! receives a request and sends a response, whereas the
//! outbound upgrade send a request and receives a response.

use futures::{future::BoxFuture, prelude::*};
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p::swarm::NegotiatedSubstream;
use smallvec::SmallVec;
use std::{fmt, io};
use tokio::sync::oneshot::{Receiver, Sender};

use super::codec::RpcCodec;
use super::{Request, RequestId, Response};
use crate::p2p::primitives::SubqueryProtocol;

/// Response substream upgrade protocol.
///
/// Receives a request and sends a response.
#[derive(Debug)]
pub struct ResponseProtocol {
    pub(crate) protocols: SmallVec<[SubqueryProtocol; 2]>,
    pub(crate) request_sender: Sender<(RequestId, Request)>,
    pub(crate) response_receiver: Receiver<Response>,
    pub(crate) request_id: RequestId,
}

impl UpgradeInfo for ResponseProtocol {
    type Info = SubqueryProtocol;
    type InfoIter = smallvec::IntoIter<[Self::Info; 2]>;

    fn protocol_info(&self) -> Self::InfoIter {
        self.protocols.clone().into_iter()
    }
}

impl InboundUpgrade<NegotiatedSubstream> for ResponseProtocol {
    type Output = bool;
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_inbound(self, mut io: NegotiatedSubstream, protocol: Self::Info) -> Self::Future {
        async move {
            let request = RpcCodec::read_request(&protocol, &mut io).await?;
            match self.request_sender.send((self.request_id, request)) {
                Ok(()) => {}
                Err(_) => panic!("Expect request receiver to be alive i.e. protocol handler to be alive.",),
            }

            if let Ok(response) = self.response_receiver.await {
                RpcCodec::write_response(&protocol, &mut io, response).await?;

                // Response was sent. Indicate to handler to emit a `ResponseSent` event.
                Ok(true)
            } else {
                io.close().await?;
                // No response was sent. Indicate to handler to emit a `ResponseOmission` event.
                Ok(false)
            }
        }
        .boxed()
    }
}

/// Request substream upgrade protocol.
///
/// Sends a request and receives a response.
pub struct RequestProtocol {
    pub(crate) protocols: SmallVec<[SubqueryProtocol; 2]>,
    pub(crate) request_id: RequestId,
    pub(crate) request: Request,
}

impl fmt::Debug for RequestProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestProtocol")
            .field("request_id", &self.request_id)
            .finish()
    }
}

impl UpgradeInfo for RequestProtocol {
    type Info = SubqueryProtocol;
    type InfoIter = smallvec::IntoIter<[Self::Info; 2]>;

    fn protocol_info(&self) -> Self::InfoIter {
        self.protocols.clone().into_iter()
    }
}

impl OutboundUpgrade<NegotiatedSubstream> for RequestProtocol {
    type Output = Response;
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(self, mut io: NegotiatedSubstream, protocol: Self::Info) -> Self::Future {
        async move {
            RpcCodec::write_request(&protocol, &mut io, self.request).await?;
            let response = RpcCodec::read_response(&protocol, &mut io).await?;
            Ok(response)
        }
        .boxed()
    }
}
