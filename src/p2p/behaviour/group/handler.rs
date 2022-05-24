use cuckoofilter::{CuckooError, CuckooFilter};
use libp2p::{
    core::{
        connection::{ConnectionId, ListenerId},
        multiaddr::{Multiaddr, Protocol as MultiAddrProtocol},
        ConnectedPoint, PeerId,
    },
    swarm::{
        dial_opts::{self, DialOpts},
        NetworkBehaviour, NetworkBehaviourAction, NotifyHandler, OneShotHandler, PollParameters,
    },
};
use rand_chacha::{
    rand_core::{RngCore, SeedableRng},
    ChaChaRng,
};
use smallvec::SmallVec;
use std::{
    collections::{
        hash_map::{DefaultHasher, HashMap},
        VecDeque,
    },
    task::{Context, Poll},
};

use super::protocol::{GroupAction, GroupActionType, GroupProtocol};
use super::{GroupConfig, GroupEvent, GroupId, GroupMessage};
use crate::p2p::primitives::{group_protocol, naive_nat, SubqueryProtocol};

/// Network behaviour that handles the Group system.
pub struct Group {
    /// Events that need to be yielded to the outside when polling.
    events: VecDeque<
        NetworkBehaviourAction<
            GroupEvent,
            OneShotHandler<GroupProtocol, GroupProtocol, InnerMessage>,
        >,
    >,
    /// the protocol current used.
    protocol: SubqueryProtocol,
    /// Configuration about the Group system.
    config: GroupConfig,
    /// List of peers the network is connected to, and the groups that they're join to.
    peers: HashMap<PeerId, (SmallVec<[GroupId; 8]>, Multiaddr)>,
    /// List of groups we're join to. Necessary to filter out messages that we receive
    /// erroneously.
    groups: HashMap<GroupId, Vec<PeerId>>,
    /// We keep track of the messages we received (in the format `hash(source ID, seq_no)`) so that
    /// we don't dispatch the same message twice if we receive it twice on the network.
    received: CuckooFilter<DefaultHasher>,
}

impl Group {
    /// Creates a `Group` with the given configuration.
    pub fn new(config: GroupConfig) -> Self {
        Group {
            config,
            protocol: group_protocol(),
            events: VecDeque::new(),
            peers: HashMap::new(),
            groups: HashMap::new(),
            received: CuckooFilter::new(),
        }
    }

    /// Add a node to the sharding group.
    pub fn add_node_to_group(&mut self, group: GroupId, peer_id: PeerId) {
        if let Some(peers) = self.groups.get(&group) {
            if !peers.contains(&peer_id) {
                self.events.push_back(NetworkBehaviourAction::Dial {
                    opts: DialOpts::peer_id(peer_id)
                        .condition(dial_opts::PeerCondition::Disconnected)
                        .build(),
                    handler: OneShotHandler::default(),
                });
            }
        }
    }

    /// Remove a node from the sharding group.
    pub fn remove_node_from_group(&mut self, group: GroupId, peer_id: PeerId) {
        if let Some(peers) = self.groups.get_mut(&group) {
            if let Some(pos) = peers.iter().position(|x| x == &peer_id) {
                peers.remove(pos);
            }
        }
    }

    /// Join to a group.
    /// Returns true if the action worked. Returns false if we were already join.
    pub fn join(&mut self, group: GroupId) -> bool {
        if self.groups.contains_key(&group) {
            return false;
        }

        for peer in self.peers.keys() {
            self.events
                .push_back(NetworkBehaviourAction::NotifyHandler {
                    peer_id: *peer,
                    handler: NotifyHandler::Any,
                    event: GroupProtocol {
                        protocol: self.protocol.clone(),
                        messages: Vec::new(),
                        actions: vec![GroupAction {
                            group: group.clone(),
                            action: GroupActionType::Join(self.config.local_port, true),
                        }],
                    },
                });
        }
        debug!("====== GROUP: joined: {}", group);
        self.groups.insert(group, vec![]);
        true
    }

    /// Leave from a group.
    pub fn leave(&mut self, group: GroupId) {
        if let Some(peers) = self.groups.remove(&group) {
            for peer_id in peers {
                self.events
                    .push_back(NetworkBehaviourAction::NotifyHandler {
                        peer_id,
                        handler: NotifyHandler::Any,
                        event: GroupProtocol {
                            protocol: self.protocol.clone(),
                            messages: Vec::new(),
                            actions: vec![GroupAction {
                                group: group.clone(),
                                action: GroupActionType::Leave,
                            }],
                        },
                    });
            }
        }
    }

    /// Broadcast a message to the network, if we're join to the group only.
    pub fn broadcast(&mut self, group: GroupId, data: impl Into<Vec<u8>>) {
        let mut rng = ChaChaRng::from_entropy();
        let mut sequence = vec![0u8; 20];
        rng.fill_bytes(&mut sequence);

        let message = GroupMessage {
            sequence,
            source: self.config.local_peer_id,
            data: data.into(),
            group: group,
        };

        if let Some(peers) = self.groups.get(&message.group) {
            if let Err(e @ CuckooError::NotEnoughSpace) = self.received.add(&message) {
                warn!(
                    "Message was added to 'received' Cuckoofilter but some \
                     other message was removed as a consequence: {}",
                    e,
                );
            }

            if self.config.subscribe_local_messages {
                self.events
                    .push_back(NetworkBehaviourAction::GenerateEvent(GroupEvent::Message(
                        message.clone(),
                    )));
            }

            for peer_id in peers.iter() {
                self.events
                    .push_back(NetworkBehaviourAction::NotifyHandler {
                        peer_id: *peer_id,
                        handler: NotifyHandler::Any,
                        event: GroupProtocol {
                            protocol: self.protocol.clone(),
                            actions: Vec::new(),
                            messages: vec![message.clone()],
                        },
                    });
            }
        }
    }
}

impl NetworkBehaviour for Group {
    type ConnectionHandler = OneShotHandler<GroupProtocol, GroupProtocol, InnerMessage>;
    type OutEvent = GroupEvent;

    fn new_handler(&mut self) -> Self::ConnectionHandler {
        OneShotHandler::default()
    }

    fn inject_new_external_addr(&mut self, addr: &Multiaddr) {
        self.config.external_addr = Some(addr.clone());
    }

    fn inject_new_listen_addr(&mut self, _id: ListenerId, addr: &Multiaddr) {
        if let Some(protocol) = addr.clone().pop() {
            match protocol {
                MultiAddrProtocol::Tcp(port) => {
                    self.config.local_port = port;
                }
                _ => {}
            }
        }
    }

    fn inject_connection_established(
        &mut self,
        id: &PeerId,
        _: &ConnectionId,
        endpoint: &ConnectedPoint,
        _: Option<&Vec<Multiaddr>>,
        _other_established: usize,
    ) {
        debug!("====== GROUP: connection established: {}", id);
        // if other_established > 0 {
        //     // We only care about the first time a peer connects.
        //     return;
        // }

        // We need to send our actions to the newly-connected node.
        if !self.peers.contains_key(id) {
            for group in self.groups.keys().cloned() {
                self.events
                    .push_back(NetworkBehaviourAction::NotifyHandler {
                        peer_id: *id,
                        handler: NotifyHandler::Any,
                        event: GroupProtocol {
                            protocol: self.protocol.clone(),
                            messages: Vec::new(),
                            actions: vec![GroupAction {
                                group,
                                action: GroupActionType::Join(self.config.local_port, true),
                            }],
                        },
                    });
            }
        }

        let addr = endpoint.get_remote_address().clone();
        self.peers.insert(*id, (SmallVec::new(), addr));
    }

    fn inject_connection_closed(
        &mut self,
        id: &PeerId,
        _: &ConnectionId,
        _: &ConnectedPoint,
        _: Self::ConnectionHandler,
        _remaining_established: usize,
    ) {
        debug!("====== GROUP: connection closed: {}", id);
        // if remaining_established > 0 {
        //     // we only care about peer disconnections
        //     return;
        // }

        let _was_in = self.peers.remove(id);
        //debug_assert!(was_in.is_some());

        // We can be disconnected by the remote in case of inactivity for example, so we always
        // try to reconnect.
        for (_group, peers) in self.groups.iter_mut() {
            if let Some(pos) = peers.iter().position(|x| x == id) {
                peers.remove(pos);
                self.events.push_back(NetworkBehaviourAction::Dial {
                    opts: DialOpts::peer_id(*id)
                        .condition(dial_opts::PeerCondition::Disconnected)
                        .build(),
                    handler: Default::default(),
                });
            }
        }
    }

    fn inject_event(&mut self, peer_id: PeerId, _connection: ConnectionId, event: InnerMessage) {
        debug!("====== GROUP: inject event: {}", peer_id);
        // We ignore successful sends or timeouts.
        let event = match event {
            InnerMessage::Rx(event) => event,
            InnerMessage::Sent => return,
        };

        // Update connected peers groups
        for action in event.actions {
            if let Some(peers) = self.groups.get_mut(&action.group) {
                debug!("====== GROUP: inject event is {:?}", action.action);
                match action.action {
                    GroupActionType::Join(port, is_request) => {
                        if is_request {
                            self.events
                                .push_back(NetworkBehaviourAction::NotifyHandler {
                                    peer_id: peer_id,
                                    handler: NotifyHandler::Any,
                                    event: GroupProtocol {
                                        protocol: self.protocol.clone(),
                                        actions: vec![GroupAction {
                                            group: action.group.clone(),
                                            action: GroupActionType::Join(
                                                self.config.local_port,
                                                false,
                                            ),
                                        }],
                                        messages: Vec::new(),
                                    },
                                });

                            // Share the group's info
                            let mut others = vec![];
                            for pid in peers.iter() {
                                if let Some((_, addr)) = self.peers.get(pid) {
                                    others.push((*pid, addr.clone()));
                                }
                            }

                            self.events
                                .push_back(NetworkBehaviourAction::NotifyHandler {
                                    peer_id: peer_id,
                                    handler: NotifyHandler::Any,
                                    event: GroupProtocol {
                                        protocol: self.protocol.clone(),
                                        actions: vec![GroupAction {
                                            group: action.group.clone(),
                                            action: GroupActionType::Sync(others),
                                        }],
                                        messages: Vec::new(),
                                    },
                                });
                        }

                        if !peers.contains(&peer_id) && self.peers.contains_key(&peer_id) {
                            let _ = self
                                .peers
                                .get_mut(&peer_id)
                                .map(|peer| peer.1 = naive_nat(&peer.1, port));
                            peers.push(peer_id);
                            self.events.push_back(NetworkBehaviourAction::GenerateEvent(
                                GroupEvent::Join {
                                    peer: peer_id,
                                    group: action.group,
                                },
                            ));
                        }
                    }
                    GroupActionType::Leave => {
                        if let Some(pos) = peers.iter().position(|x| x == &peer_id) {
                            peers.remove(pos);
                            self.events.push_back(NetworkBehaviourAction::GenerateEvent(
                                GroupEvent::Leave {
                                    peer: peer_id,
                                    group: action.group,
                                },
                            ));
                        }
                    }
                    GroupActionType::Sync(others) => {
                        debug!("***** Sync: {:?}", others);
                        for (peer_id, addr) in others {
                            if !peers.contains(&peer_id) && peer_id != self.config.local_peer_id {
                                self.events.push_back(NetworkBehaviourAction::Dial {
                                    opts: DialOpts::peer_id(peer_id)
                                        .addresses(vec![addr])
                                        .condition(dial_opts::PeerCondition::Disconnected)
                                        .build(),
                                    handler: OneShotHandler::default(),
                                });
                            }
                        }
                    }
                }
            } else {
                // TODO help build DHT
            }
        }

        // List of messages we're going to propagate on the network.
        //let mut rpcs_to_dispatch: Vec<(PeerId, GroupProtocol)> = Vec::new();

        for message in event.messages {
            if self.groups.contains_key(&message.group) {
                debug!("====== GROUP: inject event is GroupMessage");
                match self.received.test_and_add(&message) {
                    Ok(true) => {}         // Message  was added.
                    Ok(false) => continue, // Message already existed.
                    Err(e @ CuckooError::NotEnoughSpace) => {
                        // Message added, but some other removed.
                        warn!(
                            "Message was added to 'received' Cuckoofilter but some \
                         other message was removed as a consequence: {}",
                            e,
                        );
                    }
                }

                let event = GroupEvent::Message(message.clone());
                self.events
                    .push_back(NetworkBehaviourAction::GenerateEvent(event));
            };
        }
    }

    fn poll(
        &mut self,
        _: &mut Context<'_>,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>> {
        if let Some(event) = self.events.pop_front() {
            return Poll::Ready(event);
        }

        Poll::Pending
    }
}

/// Transmission between the `OneShotHandler` and the `GroupHandler`.
#[derive(Debug)]
pub enum InnerMessage {
    /// We received an RPC from a remote.
    Rx(GroupProtocol),
    /// We successfully sent an RPC request.
    Sent,
}

impl From<GroupProtocol> for InnerMessage {
    #[inline]
    fn from(data: GroupProtocol) -> InnerMessage {
        InnerMessage::Rx(data)
    }
}

impl From<()> for InnerMessage {
    #[inline]
    fn from(_: ()) -> InnerMessage {
        InnerMessage::Sent
    }
}
