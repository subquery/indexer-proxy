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

use libp2p::{
    ping::{Ping, PingConfig, PingEvent},
    NetworkBehaviour, PeerId,
};

pub mod group;
pub mod rpc;

use group::{Group, GroupConfig, GroupEvent};
use rpc::{Rpc, RpcConfig, RpcEvent};

/// Hierarchy of NetworkBehaviour.
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event")]
pub struct Behaviour {
    ping: Ping,
    pub rpc: Rpc,
    pub group: Group,
}

/// Network event.
pub enum Event {
    Ping(PingEvent),
    Rpc(RpcEvent),
    Group(GroupEvent),
}

impl From<PingEvent> for Event {
    fn from(event: PingEvent) -> Self {
        Self::Ping(event)
    }
}

impl From<RpcEvent> for Event {
    fn from(event: RpcEvent) -> Self {
        Self::Rpc(event)
    }
}

impl From<GroupEvent> for Event {
    fn from(event: GroupEvent) -> Self {
        Self::Group(event)
    }
}

/// Initiated the network behaviour.
pub fn behaviour(peer_id: PeerId) -> Behaviour {
    let ping = Ping::new(PingConfig::new().with_keep_alive(true));
    let rpc = Rpc::new(RpcConfig::default());
    let group = Group::new(GroupConfig::new(peer_id));

    Behaviour { ping, rpc, group }
}
