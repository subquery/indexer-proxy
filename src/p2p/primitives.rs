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
    pre.replace(1, |_| Some(Protocol::Tcp(port)))
        .unwrap_or(pre.clone())
}
