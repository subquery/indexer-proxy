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

use libp2p::core::multiaddr::{Multiaddr, Protocol};

/// MAX is 1024 * 1024 * 10 = 10MB
pub const MAX_NETWORK_DATA_LEN: usize = 10485760;

/// Subquery protocol name.
pub type SubqueryProtocol = String;

/// The level of support for a particular protocol.
pub enum ProtocolSupport {
    /// The protocol is only supported for inbound requests.
    Inbound,
    /// The protocol is only supported for outbound requests.
    Outbound,
    /// The protocol is supported for inbound and outbound requests.
    Full,
}

impl ProtocolSupport {
    /// Whether inbound requests are supported.
    pub fn inbound(&self) -> bool {
        match self {
            ProtocolSupport::Inbound | ProtocolSupport::Full => true,
            ProtocolSupport::Outbound => false,
        }
    }

    /// Whether outbound requests are supported.
    pub fn outbound(&self) -> bool {
        match self {
            ProtocolSupport::Outbound | ProtocolSupport::Full => true,
            ProtocolSupport::Inbound => false,
        }
    }
}

/// This node supported rpc protocols.
pub fn rpc_protocols() -> Vec<(SubqueryProtocol, ProtocolSupport)> {
    vec![("/subquery/rpc/0.0.1".to_owned(), ProtocolSupport::Full)]
}

/// This node supported group protocol.
pub fn group_protocol() -> SubqueryProtocol {
    "/subquery/group/0.0.1".to_owned()
}

/// change multiaddr to remote local port.
pub fn naive_nat(pre: &Multiaddr, port: u16) -> Multiaddr {
    pre.replace(1, |_| Some(Protocol::Tcp(port))).unwrap_or(pre.clone())
}
