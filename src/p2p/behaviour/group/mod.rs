use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Serialize};

mod handler;
mod protocol;

pub use self::handler::Group;

/// Event that can happen on the group behaviour.
#[derive(Debug)]
pub enum GroupEvent {
    /// A message has been received.
    Message(GroupMessage),

    /// A remote join to a group.
    Join {
        /// Remote that has join.
        peer: PeerId,
        /// The group id.
        group: GroupId,
    },

    /// A remote leave from a group.
    Leave {
        /// Remote that has leave.
        peer: PeerId,
        /// The group id.
        group: GroupId,
    },
}

/// A message received by the consensus system.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GroupMessage {
    /// Id of the peer that published this message.
    pub source: PeerId,

    /// The group the message send to.
    pub group: GroupId,

    /// An incrementing sequence number.
    pub sequence: Vec<u8>,

    /// Content of the message. Its meaning is out of scope of this library.
    pub data: Vec<u8>,
}

/// Configuration options for the Group.
#[derive(Debug, Clone)]
pub struct GroupConfig {
    /// Peer id of the local node. Used for the source of the messages that we publish.
    pub local_peer_id: PeerId,

    /// This node local bind port.
    pub local_port: u16,

    /// Find the external addr for this node.
    pub external_addr: Option<Multiaddr>,

    /// `true` if messages published by local node should be propagated as messages received from
    /// the network, `true` by default.
    pub subscribe_local_messages: bool,
}

impl GroupConfig {
    pub fn new(local_peer_id: PeerId) -> Self {
        Self {
            local_peer_id,
            local_port: 0,
            external_addr: None,
            subscribe_local_messages: true,
        }
    }
}

/// GroupId, every Blockchain or Dapp has different GroupId.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GroupId(String);

impl GroupId {
    /// Returns the id of the project.
    #[inline]
    pub fn id(&self) -> &str {
        &self.0
    }

    pub fn new<S>(name: S) -> GroupId
    where
        S: Into<String>,
    {
        GroupId(name.into())
    }
}

impl std::fmt::Display for GroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<GroupId> for String {
    fn from(group: GroupId) -> String {
        group.0
    }
}
