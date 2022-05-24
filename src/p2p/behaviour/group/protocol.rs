use futures::{future::BoxFuture, prelude::*};
use libp2p::core::{upgrade, InboundUpgrade, Multiaddr, OutboundUpgrade, PeerId, UpgradeInfo};
use serde::{Deserialize, Serialize};
use std::{io, iter};

use super::{GroupId, GroupMessage};
use crate::p2p::primitives::{group_protocol, SubqueryProtocol, MAX_NETWORK_DATA_LEN};

/// Implementation of `ConnectionUpgrade` for the floodsub protocol.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GroupProtocol {
    /// The protocol current used.
    pub protocol: SubqueryProtocol,
    /// List of messages that were part of this RPC query.
    pub messages: Vec<GroupMessage>,
    /// List of actions.
    pub actions: Vec<GroupAction>,
}

impl Default for GroupProtocol {
    fn default() -> Self {
        GroupProtocol {
            protocol: group_protocol(),
            messages: vec![],
            actions: vec![],
        }
    }
}

impl UpgradeInfo for GroupProtocol {
    type Info = SubqueryProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(self.protocol.clone())
    }
}

impl<T> InboundUpgrade<T> for GroupProtocol
where
    T: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = GroupProtocol;
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_inbound(self, mut io: T, _: Self::Info) -> Self::Future {
        async move {
            let packet = upgrade::read_length_prefixed(&mut io, MAX_NETWORK_DATA_LEN).await?;
            let event: GroupProtocol = bincode::deserialize(&packet)
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
            Ok(event)
        }
        .boxed()
    }
}

impl<T> OutboundUpgrade<T> for GroupProtocol
where
    T: AsyncWrite + AsyncRead + Send + Unpin + 'static,
{
    type Output = ();
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(self, mut io: T, _: Self::Info) -> Self::Future {
        async move {
            let bytes = bincode::serialize(&self)
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
            upgrade::write_length_prefixed(&mut io, bytes).await?;
            io.close().await?;

            Ok(())
        }
        .boxed()
    }
}

/// A group action received by the consensus system.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GroupAction {
    /// Action to perform.
    pub action: GroupActionType,
    /// The group from which to subscribe or unsubscribe.
    pub group: GroupId,
}

/// Action that a peer wants to perform.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GroupActionType {
    /// The remote wants to join to the given group.
    /// params is local port, is_request.
    Join(u16, bool),
    /// The remote wants to leave from the given group.
    Leave,
    /// Sync the group other peers info.
    Sync(Vec<(PeerId, Multiaddr)>),
}
