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
