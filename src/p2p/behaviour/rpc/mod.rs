use libp2p::{
    core::{connection::ConnectionId, ConnectedPoint, Multiaddr, PeerId},
    swarm::{
        dial_opts::{self, DialOpts},
        DialError, IntoConnectionHandler, NetworkBehaviour, NetworkBehaviourAction, NotifyHandler,
        PollParameters,
    },
};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    sync::{atomic::AtomicU64, Arc},
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::oneshot::Sender;

use crate::p2p::primitives::{rpc_protocols, SubqueryProtocol};

mod codec;
mod handler;
mod protocol;

use handler::{RpcHandler, RpcHandlerEvent};
use protocol::RequestProtocol;

pub type RequestId = u64;

/// Http Request/Response method.
#[derive(Debug, Deserialize, Serialize)]
pub enum HttpMethod {
    Get,
    Post,
}

impl From<&str> for HttpMethod {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "post" => HttpMethod::Post,
            _ => HttpMethod::Get,
        }
    }
}

/// Rpc Request type.
#[derive(Debug, Deserialize, Serialize)]
pub enum Request {
    /// consumer query info to indexer.
    /// http method, http query path, json data. data sign.
    Query(HttpMethod, String, String, String),
    /// state channel info.
    StateChannel(String),
}

/// Rpc Request type.
#[derive(Debug, Deserialize, Serialize)]
pub enum Response {
    /// data query from indexer.
    RawData(String),
    /// sign with query.
    Sign(String),
    /// data query from indexer and sign with data.
    Data(String, String),
    /// state channel info.
    StateChannel(String),
    /// error response.
    Error(String),
}

impl Response {
    pub fn with_sign(self, sign: Response) -> Response {
        let data = match self {
            Response::RawData(data) => data,
            _ => return self,
        };
        let sign = match sign {
            Response::Sign(sign) => sign,
            _ => "".to_owned(),
        };
        Response::Data(data, sign)
    }
}

/// An inbound request or response.
#[derive(Debug)]
pub enum RpcMessage {
    /// A request message.
    Request {
        /// The ID of this request.
        request_id: RequestId,
        /// The request message.
        request: Request,
    },
    /// A response message.
    Response {
        /// The ID of the request that produced this response.
        request_id: RequestId,
        /// The response message.
        response: Response,
    },
}

/// The events emitted by a [`Rpc`] protocol.
#[derive(Debug)]
pub enum RpcEvent {
    /// An incoming message (request or response).
    Message {
        /// The peer who sent the message.
        peer: PeerId,
        /// The incoming message.
        message: RpcMessage,
    },
    /// An outbound request failed.
    OutboundFailure {
        /// The peer to whom the request was sent.
        peer: PeerId,
        /// The (local) ID of the failed request.
        request_id: RequestId,
        /// The error that occurred.
        error: OutboundFailure,
    },
    /// An inbound request failed.
    InboundFailure {
        /// The peer from whom the request was received.
        peer: PeerId,
        /// The ID of the failed inbound request.
        request_id: RequestId,
        /// The error that occurred.
        error: InboundFailure,
    },
    /// A response to an inbound request has been sent.
    ///
    /// When this event is received, the response has been flushed on
    /// the underlying transport connection.
    ResponseSent {
        /// The peer to whom the response was sent.
        peer: PeerId,
        /// The ID of the inbound request whose response was sent.
        request_id: RequestId,
    },
}

/// Possible failures occurring in the context of sending
/// an outbound request and receiving the response.
#[derive(Debug, Clone, PartialEq)]
pub enum OutboundFailure {
    /// The request could not be sent because a dialing attempt failed.
    DialFailure,
    /// The request timed out before a response was received.
    ///
    /// It is not known whether the request may have been
    /// received (and processed) by the remote peer.
    Timeout,
    /// The connection closed before a response was received.
    ///
    /// It is not known whether the request may have been
    /// received (and processed) by the remote peer.
    ConnectionClosed,
    /// The remote supports none of the requested protocols.
    UnsupportedProtocols,
}

impl fmt::Display for OutboundFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutboundFailure::DialFailure => write!(f, "Failed to dial the requested peer"),
            OutboundFailure::Timeout => write!(f, "Timeout while waiting for a response"),
            OutboundFailure::ConnectionClosed => {
                write!(f, "Connection was closed before a response was received")
            }
            OutboundFailure::UnsupportedProtocols => {
                write!(f, "The remote supports none of the requested protocols")
            }
        }
    }
}

impl std::error::Error for OutboundFailure {}

/// Possible failures occurring in the context of receiving an
/// inbound request and sending a response.
#[derive(Debug, Clone, PartialEq)]
pub enum InboundFailure {
    /// The inbound request timed out, either while reading the
    /// incoming request or before a response is sent, e.g. if
    /// [`Rpc::send_response`] is not called in a
    /// timely manner.
    Timeout,
    /// The connection closed before a response could be send.
    ConnectionClosed,
    /// The local peer supports none of the protocols requested
    /// by the remote.
    UnsupportedProtocols,
    /// The local peer failed to respond to an inbound request
    /// due to the [`ResponseChannel`] being dropped instead of
    /// being passed to [`Rpc::send_response`].
    ResponseOmission,
}

impl fmt::Display for InboundFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InboundFailure::Timeout => {
                write!(f, "Timeout while receiving request or sending response")
            }
            InboundFailure::ConnectionClosed => {
                write!(f, "Connection was closed before a response could be sent")
            }
            InboundFailure::UnsupportedProtocols => write!(
                f,
                "The local peer supports none of the protocols requested by the remote"
            ),
            InboundFailure::ResponseOmission => write!(
                f,
                "The response channel was dropped without sending a response to the remote"
            ),
        }
    }
}

impl std::error::Error for InboundFailure {}

/// The configuration for a `Rpc` protocol.
#[derive(Debug, Clone)]
pub struct RpcConfig {
    request_timeout: Duration,
    connection_keep_alive: Duration,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            connection_keep_alive: Duration::from_secs(10),
            request_timeout: Duration::from_secs(10),
        }
    }
}

impl RpcConfig {
    /// Sets the keep-alive timeout of idle connections.
    pub fn set_connection_keep_alive(&mut self, v: Duration) -> &mut Self {
        self.connection_keep_alive = v;
        self
    }

    /// Sets the timeout for inbound and outbound requests.
    pub fn set_request_timeout(&mut self, v: Duration) -> &mut Self {
        self.request_timeout = v;
        self
    }
}

/// A request/response protocol for some message codec.
pub struct Rpc {
    /// The supported inbound protocols.
    inbound_protocols: SmallVec<[SubqueryProtocol; 2]>,
    /// The supported outbound protocols.
    outbound_protocols: SmallVec<[SubqueryProtocol; 2]>,
    /// The next (local) request ID.
    next_request_id: RequestId,
    /// The next (inbound) request ID.
    next_inbound_id: Arc<AtomicU64>,
    /// The protocol configuration.
    config: RpcConfig,
    /// Pending events to return from `poll`.
    pending_events: VecDeque<NetworkBehaviourAction<RpcEvent, RpcHandler>>,
    /// The currently connected peers, their pending outbound and inbound
    /// responses and their known, reachable addresses, if any.
    connected: HashMap<PeerId, SmallVec<[Connection; 2]>>,
    /// Externally managed addresses via `add_address` and `remove_address`.
    addresses: HashMap<PeerId, SmallVec<[Multiaddr; 6]>>,
    /// Requests that have not yet been sent and are waiting for a connection
    /// to be established.
    pending_outbound_requests: HashMap<PeerId, SmallVec<[RequestProtocol; 10]>>,
    /// Response channel waiting for outside handle it.
    waiting_requests: HashMap<RequestId, Sender<Response>>,
}

impl Rpc {
    /// Creates a new `Rpc` behaviour for the given
    /// protocols and configuration.
    pub fn new(cfg: RpcConfig) -> Self {
        let protocols = rpc_protocols();
        let mut inbound_protocols = SmallVec::new();
        let mut outbound_protocols = SmallVec::new();
        for (p, s) in protocols {
            if s.inbound() {
                inbound_protocols.push(p.clone());
            }
            if s.outbound() {
                outbound_protocols.push(p.clone());
            }
        }
        Rpc {
            inbound_protocols,
            outbound_protocols,
            next_request_id: 1, // RequestId
            next_inbound_id: Arc::new(AtomicU64::new(1)),
            config: cfg,
            pending_events: VecDeque::new(),
            connected: HashMap::new(),
            pending_outbound_requests: HashMap::new(),
            addresses: HashMap::new(),
            waiting_requests: HashMap::new(),
        }
    }

    /// Initiates sending a request.
    ///
    /// If the targeted peer is currently not connected, a dialing
    /// attempt is initiated and the request is sent as soon as a
    /// connection is established.
    ///
    /// > **Note**: In order for such a dialing attempt to succeed,
    /// > the `RequestResonse` protocol must either be embedded
    /// > in another `NetworkBehaviour` that provides peer and
    /// > address discovery, or known addresses of peers must be
    /// > managed via [`Rpc::add_address`] and
    /// > [`Rpc::remove_address`].
    pub fn request(&mut self, peer: PeerId, request: Request) -> RequestId {
        let request_id = self.next_request_id();
        let request = RequestProtocol {
            request_id,
            protocols: self.outbound_protocols.clone(),
            request,
        };

        if let Some(request) = self.try_send_request(&peer, request) {
            let handler = self.new_handler();
            self.pending_events.push_back(NetworkBehaviourAction::Dial {
                opts: DialOpts::peer_id(peer)
                    .condition(dial_opts::PeerCondition::Disconnected)
                    .build(),
                handler,
            });
            self.pending_outbound_requests
                .entry(peer)
                .or_default()
                .push(request);
        }

        request_id
    }

    /// Initiates sending a response to an inbound request.
    ///
    /// If the [`ResponseChannel`] is already closed due to a timeout or the
    /// connection being closed, the response is returned as an `Err` for
    /// further handling. Once the response has been successfully sent on the
    /// corresponding connection, [`RpcEvent::ResponseSent`] is
    /// emitted. In all other cases [`RpcEvent::InboundFailure`]
    /// will be or has been emitted.
    ///
    /// The provided `ResponseChannel` is obtained from an inbound
    /// [`RpcMessage::Request`].
    pub fn response(&mut self, uid: RequestId, response: Response) -> Result<(), Response> {
        if let Some(channel) = self.waiting_requests.remove(&uid) {
            channel.send(response)
        } else {
            Ok(())
        }
    }

    /// Adds a known address for a peer that can be used for
    /// dialing attempts by the `Swarm`, i.e. is returned
    /// by [`NetworkBehaviour::addresses_of_peer`].
    ///
    /// Addresses added in this way are only removed by `remove_address`.
    pub fn add_address(&mut self, peer: &PeerId, address: Multiaddr) {
        self.addresses.entry(*peer).or_default().push(address);
    }

    /// Removes an address of a peer previously added via `add_address`.
    pub fn remove_address(&mut self, peer: &PeerId, address: &Multiaddr) {
        let mut last = false;
        if let Some(addresses) = self.addresses.get_mut(peer) {
            addresses.retain(|a| a != address);
            last = addresses.is_empty();
        }
        if last {
            self.addresses.remove(peer);
        }
    }

    /// Checks whether a peer is currently connected.
    pub fn is_connected(&self, peer: &PeerId) -> bool {
        if let Some(connections) = self.connected.get(peer) {
            !connections.is_empty()
        } else {
            false
        }
    }

    /// Checks whether an outbound request to the peer with the provided
    /// [`PeerId`] initiated by [`Rpc::send_request`] is still
    /// pending, i.e. waiting for a response.
    pub fn is_pending_outbound(&self, peer: &PeerId, request_id: &RequestId) -> bool {
        // Check if request is already sent on established connection.
        let est_conn = self
            .connected
            .get(peer)
            .map(|cs| {
                cs.iter()
                    .any(|c| c.pending_inbound_responses.contains(request_id))
            })
            .unwrap_or(false);
        // Check if request is still pending to be sent.
        let pen_conn = self
            .pending_outbound_requests
            .get(peer)
            .map(|rps| rps.iter().any(|rp| rp.request_id == *request_id))
            .unwrap_or(false);

        est_conn || pen_conn
    }

    /// Checks whether an inbound request from the peer with the provided
    /// [`PeerId`] is still pending, i.e. waiting for a response by the local
    /// node through [`Rpc::send_response`].
    pub fn is_pending_inbound(&self, peer: &PeerId, request_id: &RequestId) -> bool {
        self.connected
            .get(peer)
            .map(|cs| {
                cs.iter()
                    .any(|c| c.pending_outbound_responses.contains(request_id))
            })
            .unwrap_or(false)
    }

    /// Returns the next request ID.
    fn next_request_id(&mut self) -> RequestId {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        request_id
    }

    /// Tries to send a request by queueing an appropriate event to be
    /// emitted to the `Swarm`. If the peer is not currently connected,
    /// the given request is return unchanged.
    fn try_send_request(
        &mut self,
        peer: &PeerId,
        request: RequestProtocol,
    ) -> Option<RequestProtocol> {
        if let Some(connections) = self.connected.get_mut(peer) {
            if connections.is_empty() {
                return Some(request);
            }
            debug!("=========== GOT CONNECTIONS");
            let ix = (request.request_id as usize) % connections.len();
            let conn = &mut connections[ix];
            conn.pending_inbound_responses.insert(request.request_id);
            self.pending_events
                .push_back(NetworkBehaviourAction::NotifyHandler {
                    peer_id: *peer,
                    handler: NotifyHandler::One(conn.id),
                    event: request,
                });
            None
        } else {
            Some(request)
        }
    }

    /// Remove pending outbound response for the given peer and connection.
    ///
    /// Returns `true` if the provided connection to the given peer is still
    /// alive and the [`RequestId`] was previously present and is now removed.
    /// Returns `false` otherwise.
    fn remove_pending_outbound_response(
        &mut self,
        peer: &PeerId,
        connection: ConnectionId,
        request: RequestId,
    ) -> bool {
        self.get_connection_mut(peer, connection)
            .map(|c| c.pending_outbound_responses.remove(&request))
            .unwrap_or(false)
    }

    /// Remove pending inbound response for the given peer and connection.
    ///
    /// Returns `true` if the provided connection to the given peer is still
    /// alive and the [`RequestId`] was previously present and is now removed.
    /// Returns `false` otherwise.
    fn remove_pending_inbound_response(
        &mut self,
        peer: &PeerId,
        connection: ConnectionId,
        request: &RequestId,
    ) -> bool {
        self.get_connection_mut(peer, connection)
            .map(|c| c.pending_inbound_responses.remove(request))
            .unwrap_or(false)
    }

    /// Returns a mutable reference to the connection in `self.connected`
    /// corresponding to the given [`PeerId`] and [`ConnectionId`].
    fn get_connection_mut(
        &mut self,
        peer: &PeerId,
        connection: ConnectionId,
    ) -> Option<&mut Connection> {
        self.connected
            .get_mut(peer)
            .and_then(|connections| connections.iter_mut().find(|c| c.id == connection))
    }
}

impl NetworkBehaviour for Rpc {
    type ConnectionHandler = RpcHandler;
    type OutEvent = RpcEvent;

    fn new_handler(&mut self) -> Self::ConnectionHandler {
        RpcHandler::new(
            self.inbound_protocols.clone(),
            self.config.connection_keep_alive,
            self.config.request_timeout,
            self.next_inbound_id.clone(),
        )
    }

    fn addresses_of_peer(&mut self, peer: &PeerId) -> Vec<Multiaddr> {
        let mut addresses = Vec::new();
        if let Some(connections) = self.connected.get(peer) {
            addresses.extend(connections.iter().filter_map(|c| c.address.clone()))
        }
        if let Some(more) = self.addresses.get(peer) {
            addresses.extend(more.into_iter().cloned());
        }
        addresses
    }

    fn inject_address_change(
        &mut self,
        peer: &PeerId,
        conn: &ConnectionId,
        _old: &ConnectedPoint,
        new: &ConnectedPoint,
    ) {
        let new_address = match new {
            ConnectedPoint::Dialer { address, .. } => Some(address.clone()),
            ConnectedPoint::Listener { .. } => None,
        };
        let connections = self
            .connected
            .get_mut(peer)
            .expect("Address change can only happen on an established connection.");

        let connection = connections
            .iter_mut()
            .find(|c| &c.id == conn)
            .expect("Address change can only happen on an established connection.");
        connection.address = new_address;
    }

    fn inject_connection_established(
        &mut self,
        peer: &PeerId,
        conn: &ConnectionId,
        endpoint: &ConnectedPoint,
        _errors: Option<&Vec<Multiaddr>>,
        other_established: usize,
    ) {
        debug!("------ RPC: connection established: {}", peer);
        let address = match endpoint {
            ConnectedPoint::Dialer { address, .. } => Some(address.clone()),
            ConnectedPoint::Listener { .. } => None,
        };
        self.connected
            .entry(*peer)
            .or_default()
            .push(Connection::new(*conn, address));

        if other_established == 0 {
            if let Some(pending) = self.pending_outbound_requests.remove(peer) {
                for request in pending {
                    let request = self.try_send_request(peer, request);
                    assert!(request.is_none());
                }
            }
        }
    }

    fn inject_connection_closed(
        &mut self,
        peer_id: &PeerId,
        conn: &ConnectionId,
        _: &ConnectedPoint,
        _: <Self::ConnectionHandler as IntoConnectionHandler>::Handler,
        remaining_established: usize,
    ) {
        debug!("------ RPC: connection closed: {}", peer_id);
        let connections = self
            .connected
            .get_mut(peer_id)
            .expect("Expected some established connection to peer before closing.");

        let connection = connections
            .iter()
            .position(|c| &c.id == conn)
            .map(|p: usize| connections.remove(p))
            .expect("Expected connection to be established before closing.");

        debug_assert_eq!(connections.is_empty(), remaining_established == 0);
        if connections.is_empty() {
            self.connected.remove(peer_id);
        }

        for request_id in connection.pending_outbound_responses {
            self.pending_events
                .push_back(NetworkBehaviourAction::GenerateEvent(
                    RpcEvent::InboundFailure {
                        peer: *peer_id,
                        request_id,
                        error: InboundFailure::ConnectionClosed,
                    },
                ));
        }

        for request_id in connection.pending_inbound_responses {
            self.pending_events
                .push_back(NetworkBehaviourAction::GenerateEvent(
                    RpcEvent::OutboundFailure {
                        peer: *peer_id,
                        request_id,
                        error: OutboundFailure::ConnectionClosed,
                    },
                ));
        }
    }

    fn inject_dial_failure(
        &mut self,
        peer: Option<PeerId>,
        _: Self::ConnectionHandler,
        _: &DialError,
    ) {
        debug!("------ RPC: behaviour inject dial failure");
        if let Some(peer) = peer {
            // If there are pending outgoing requests when a dial failure occurs,
            // it is implied that we are not connected to the peer, since pending
            // outgoing requests are drained when a connection is established and
            // only created when a peer is not connected when a request is made.
            // Thus these requests must be considered failed, even if there is
            // another, concurrent dialing attempt ongoing.
            if let Some(pending) = self.pending_outbound_requests.remove(&peer) {
                for request in pending {
                    self.pending_events
                        .push_back(NetworkBehaviourAction::GenerateEvent(
                            RpcEvent::OutboundFailure {
                                peer: peer,
                                request_id: request.request_id,
                                error: OutboundFailure::DialFailure,
                            },
                        ));
                }
            }
        }
    }

    fn inject_event(&mut self, peer: PeerId, connection: ConnectionId, event: RpcHandlerEvent) {
        debug!("------ RPC: inject event: {}", peer);
        match event {
            RpcHandlerEvent::Response {
                request_id,
                response,
            } => {
                let removed = self.remove_pending_inbound_response(&peer, connection, &request_id);
                debug_assert!(
                    removed,
                    "Expect request_id to be pending before receiving response.",
                );

                let message = RpcMessage::Response {
                    request_id,
                    response,
                };
                self.pending_events
                    .push_back(NetworkBehaviourAction::GenerateEvent(RpcEvent::Message {
                        peer,
                        message,
                    }));
            }
            RpcHandlerEvent::Request {
                request_id,
                request,
                channel,
            } => {
                self.waiting_requests.insert(request_id, channel);
                let message = RpcMessage::Request {
                    request_id,
                    request,
                };
                self.pending_events
                    .push_back(NetworkBehaviourAction::GenerateEvent(RpcEvent::Message {
                        peer,
                        message,
                    }));

                match self.get_connection_mut(&peer, connection) {
                    Some(connection) => {
                        let inserted = connection.pending_outbound_responses.insert(request_id);
                        debug_assert!(inserted, "Expect id of new request to be unknown.");
                    }
                    // Connection closed after `RpcEvent::Request` has been emitted.
                    None => {
                        self.pending_events
                            .push_back(NetworkBehaviourAction::GenerateEvent(
                                RpcEvent::InboundFailure {
                                    peer,
                                    request_id,
                                    error: InboundFailure::ConnectionClosed,
                                },
                            ));
                    }
                }
            }
            RpcHandlerEvent::ResponseSent(request_id) => {
                let removed = self.remove_pending_outbound_response(&peer, connection, request_id);
                debug_assert!(
                    removed,
                    "Expect request_id to be pending before response is sent."
                );

                self.pending_events
                    .push_back(NetworkBehaviourAction::GenerateEvent(
                        RpcEvent::ResponseSent { peer, request_id },
                    ));
            }
            RpcHandlerEvent::ResponseOmission(request_id) => {
                let removed = self.remove_pending_outbound_response(&peer, connection, request_id);
                debug_assert!(
                    removed,
                    "Expect request_id to be pending before response is omitted.",
                );

                self.pending_events
                    .push_back(NetworkBehaviourAction::GenerateEvent(
                        RpcEvent::InboundFailure {
                            peer,
                            request_id,
                            error: InboundFailure::ResponseOmission,
                        },
                    ));
            }
            RpcHandlerEvent::OutboundTimeout(request_id) => {
                let removed = self.remove_pending_inbound_response(&peer, connection, &request_id);
                debug_assert!(
                    removed,
                    "Expect request_id to be pending before request times out."
                );

                self.pending_events
                    .push_back(NetworkBehaviourAction::GenerateEvent(
                        RpcEvent::OutboundFailure {
                            peer,
                            request_id,
                            error: OutboundFailure::Timeout,
                        },
                    ));
            }
            RpcHandlerEvent::InboundTimeout(request_id) => {
                // Note: `RpcHandlerEvent::InboundTimeout` is emitted both for timing
                // out to receive the request and for timing out sending the response. In the former
                // case the request is never added to `pending_outbound_responses` and thus one can
                // not assert the request_id to be present before removing it.
                self.remove_pending_outbound_response(&peer, connection, request_id);

                self.pending_events
                    .push_back(NetworkBehaviourAction::GenerateEvent(
                        RpcEvent::InboundFailure {
                            peer,
                            request_id,
                            error: InboundFailure::Timeout,
                        },
                    ));
            }
            RpcHandlerEvent::OutboundUnsupportedProtocols(request_id) => {
                let removed = self.remove_pending_inbound_response(&peer, connection, &request_id);
                debug_assert!(
                    removed,
                    "Expect request_id to be pending before failing to connect.",
                );

                self.pending_events
                    .push_back(NetworkBehaviourAction::GenerateEvent(
                        RpcEvent::OutboundFailure {
                            peer,
                            request_id,
                            error: OutboundFailure::UnsupportedProtocols,
                        },
                    ));
            }
            RpcHandlerEvent::InboundUnsupportedProtocols(request_id) => {
                // Note: No need to call `self.remove_pending_outbound_response`,
                // `RpcHandlerEvent::Request` was never emitted for this request and
                // thus request was never added to `pending_outbound_responses`.
                self.pending_events
                    .push_back(NetworkBehaviourAction::GenerateEvent(
                        RpcEvent::InboundFailure {
                            peer,
                            request_id,
                            error: InboundFailure::UnsupportedProtocols,
                        },
                    ));
            }
        }
    }

    fn poll(
        &mut self,
        _: &mut Context<'_>,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>> {
        if let Some(ev) = self.pending_events.pop_front() {
            return Poll::Ready(ev);
        } else if self.pending_events.capacity() > EMPTY_QUEUE_SHRINK_THRESHOLD {
            self.pending_events.shrink_to_fit();
        }

        Poll::Pending
    }
}

/// Internal threshold for when to shrink the capacity
/// of empty queues. If the capacity of an empty queue
/// exceeds this threshold, the associated memory is
/// released.
const EMPTY_QUEUE_SHRINK_THRESHOLD: usize = 100;

/// Internal information tracked for an established connection.
struct Connection {
    id: ConnectionId,
    address: Option<Multiaddr>,
    /// Pending outbound responses where corresponding inbound requests have
    /// been received on this connection and emitted via `poll` but have not yet
    /// been answered.
    pending_outbound_responses: HashSet<RequestId>,
    /// Pending inbound responses for previously sent requests on this
    /// connection.
    pending_inbound_responses: HashSet<RequestId>,
}

impl Connection {
    fn new(id: ConnectionId, address: Option<Multiaddr>) -> Self {
        Self {
            id,
            address,
            pending_outbound_responses: Default::default(),
            pending_inbound_responses: Default::default(),
        }
    }
}
