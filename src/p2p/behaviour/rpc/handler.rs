use futures::{future::BoxFuture, prelude::*, stream::FuturesUnordered};
use instant::Instant;
use libp2p::{
    core::upgrade::{NegotiationError, UpgradeError},
    swarm::{
        handler::{ConnectionHandler, ConnectionHandlerEvent, ConnectionHandlerUpgrErr, KeepAlive},
        SubstreamProtocol,
    },
};
use smallvec::SmallVec;
use std::{
    collections::VecDeque,
    fmt, io,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::oneshot::{channel, error::RecvError, Sender};

use super::protocol::{RequestProtocol, ResponseProtocol};
use super::{Request, RequestId, Response, EMPTY_QUEUE_SHRINK_THRESHOLD};
use crate::p2p::primitives::SubqueryProtocol;

/// A connection handler of a `Rpc` protocol.
pub struct RpcHandler {
    /// The supported inbound protocols.
    inbound_protocols: SmallVec<[SubqueryProtocol; 2]>,
    /// The keep-alive timeout of idle connections. A connection is considered
    /// idle if there are no outbound substreams.
    keep_alive_timeout: Duration,
    /// The timeout for inbound and outbound substreams (i.e. request
    /// and response processing).
    substream_timeout: Duration,
    /// The current connection keep-alive.
    keep_alive: KeepAlive,
    /// A pending fatal error that results in the connection being closed.
    pending_error: Option<ConnectionHandlerUpgrErr<io::Error>>,
    /// Queue of events to emit in `poll()`.
    pending_events: VecDeque<RpcHandlerEvent>,
    /// Outbound upgrades waiting to be emitted as an `OutboundSubstreamRequest`.
    outbound: VecDeque<RequestProtocol>,
    /// Inbound upgrades waiting for the incoming request.
    inbound: FuturesUnordered<
        BoxFuture<'static, Result<((RequestId, Request), Sender<Response>), RecvError>>,
    >,
    inbound_request_id: Arc<AtomicU64>,
}

impl RpcHandler {
    pub(super) fn new(
        inbound_protocols: SmallVec<[SubqueryProtocol; 2]>,
        keep_alive_timeout: Duration,
        substream_timeout: Duration,
        inbound_request_id: Arc<AtomicU64>,
    ) -> Self {
        Self {
            inbound_protocols,
            keep_alive: KeepAlive::Yes,
            keep_alive_timeout,
            substream_timeout,
            outbound: VecDeque::new(),
            inbound: FuturesUnordered::new(),
            pending_events: VecDeque::new(),
            pending_error: None,
            inbound_request_id,
        }
    }
}

/// The events emitted by the [`RpcHandler`].
#[doc(hidden)]
pub enum RpcHandlerEvent {
    /// A request has been received.
    Request {
        request_id: RequestId,
        request: Request,
        channel: Sender<Response>,
    },
    /// A response has been received.
    Response {
        request_id: RequestId,
        response: Response,
    },
    /// A response to an inbound request has been sent.
    ResponseSent(RequestId),
    /// A response to an inbound request was omitted as a result
    /// of dropping the response `sender` of an inbound `Request`.
    ResponseOmission(RequestId),
    /// An outbound request timed out while sending the request
    /// or waiting for the response.
    OutboundTimeout(RequestId),
    /// An outbound request failed to negotiate a mutually supported protocol.
    OutboundUnsupportedProtocols(RequestId),
    /// An inbound request timed out while waiting for the request
    /// or sending the response.
    InboundTimeout(RequestId),
    /// An inbound request failed to negotiate a mutually supported protocol.
    InboundUnsupportedProtocols(RequestId),
}

impl fmt::Debug for RpcHandlerEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpcHandlerEvent::Request {
                request_id,
                request: _,
                channel: _,
            } => f
                .debug_struct("RpcHandlerEvent::Request")
                .field("request_id", request_id)
                .finish(),
            RpcHandlerEvent::Response {
                request_id,
                response: _,
            } => f
                .debug_struct("RpcHandlerEvent::Response")
                .field("request_id", request_id)
                .finish(),
            RpcHandlerEvent::ResponseSent(request_id) => f
                .debug_tuple("RpcHandlerEvent::ResponseSent")
                .field(request_id)
                .finish(),
            RpcHandlerEvent::ResponseOmission(request_id) => f
                .debug_tuple("RpcHandlerEvent::ResponseOmission")
                .field(request_id)
                .finish(),
            RpcHandlerEvent::OutboundTimeout(request_id) => f
                .debug_tuple("RpcHandlerEvent::OutboundTimeout")
                .field(request_id)
                .finish(),
            RpcHandlerEvent::OutboundUnsupportedProtocols(request_id) => f
                .debug_tuple("RpcHandlerEvent::OutboundUnsupportedProtocols")
                .field(request_id)
                .finish(),
            RpcHandlerEvent::InboundTimeout(request_id) => f
                .debug_tuple("RpcHandlerEvent::InboundTimeout")
                .field(request_id)
                .finish(),
            RpcHandlerEvent::InboundUnsupportedProtocols(request_id) => f
                .debug_tuple("RpcHandlerEvent::InboundUnsupportedProtocols")
                .field(request_id)
                .finish(),
        }
    }
}

impl ConnectionHandler for RpcHandler {
    type InEvent = RequestProtocol;
    type OutEvent = RpcHandlerEvent;
    type Error = ConnectionHandlerUpgrErr<io::Error>;
    type InboundProtocol = ResponseProtocol;
    type OutboundProtocol = RequestProtocol;
    type OutboundOpenInfo = RequestId;
    type InboundOpenInfo = RequestId;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        debug!("------ RPC: listen protocol");
        // A channel for notifying the handler when the inbound
        // upgrade received the request.
        let (rq_send, rq_recv) = channel();

        // A channel for notifying the inbound upgrade when the
        // response is sent.
        let (rs_send, rs_recv) = channel();

        let request_id = self.inbound_request_id.fetch_add(1, Ordering::Relaxed);

        // By keeping all I/O inside the `ResponseProtocol` and thus the
        // inbound substream upgrade via above channels, we ensure that it
        // is all subject to the configured timeout without extra bookkeeping
        // for inbound substreams as well as their timeouts and also make the
        // implementation of inbound and outbound upgrades symmetric in
        // this sense.
        let proto = ResponseProtocol {
            protocols: self.inbound_protocols.clone(),
            request_sender: rq_send,
            response_receiver: rs_recv,
            request_id,
        };

        // The handler waits for the request to come in. It then emits
        // `RpcHandlerEvent::Request` together with a
        // `ResponseChannel`.
        self.inbound
            .push(rq_recv.map_ok(move |rq| (rq, rs_send)).boxed());

        SubstreamProtocol::new(proto, request_id).with_timeout(self.substream_timeout)
    }

    fn inject_fully_negotiated_inbound(&mut self, sent: bool, request_id: RequestId) {
        debug!("------ RPC: inject_fully_negotiated_inbound");
        if sent {
            self.pending_events
                .push_back(RpcHandlerEvent::ResponseSent(request_id))
        } else {
            self.pending_events
                .push_back(RpcHandlerEvent::ResponseOmission(request_id))
        }
    }

    fn inject_fully_negotiated_outbound(&mut self, response: Response, request_id: RequestId) {
        debug!("------ RPC: inject_fully_negotiated_outbound");
        self.pending_events.push_back(RpcHandlerEvent::Response {
            request_id,
            response,
        });
    }

    fn inject_event(&mut self, request: Self::InEvent) {
        debug!("------ RPC: inject_event");
        self.keep_alive = KeepAlive::Yes;
        self.outbound.push_back(request);
    }

    fn inject_dial_upgrade_error(
        &mut self,
        info: RequestId,
        error: ConnectionHandlerUpgrErr<io::Error>,
    ) {
        debug!("------ RPC: inject_dial_upgrade_error");
        match error {
            ConnectionHandlerUpgrErr::Timeout => {
                debug!("------ RPC: inject_dial_upgrade_error timeout");
                self.pending_events
                    .push_back(RpcHandlerEvent::OutboundTimeout(info));
            }
            ConnectionHandlerUpgrErr::Upgrade(UpgradeError::Select(NegotiationError::Failed)) => {
                debug!("------ RPC: inject_dial_upgrade_error OutboundUnsupportedProtocols");
                // The remote merely doesn't support the protocol(s) we requested.
                // This is no reason to close the connection, which may
                // successfully communicate with other protocols already.
                // An event is reported to permit user code to react to the fact that
                // the remote peer does not support the requested protocol(s).
                self.pending_events
                    .push_back(RpcHandlerEvent::OutboundUnsupportedProtocols(info));
            }
            _ => {
                debug!("------ RPC: inject_dial_upgrade_error Others: {}", error);
                // Anything else is considered a fatal error or misbehaviour of
                // the remote peer and results in closing the connection.
                self.pending_error = Some(error);
            }
        }
    }

    fn inject_listen_upgrade_error(
        &mut self,
        info: RequestId,
        error: ConnectionHandlerUpgrErr<io::Error>,
    ) {
        debug!("------ RPC: inject_listen_upgrade_error");
        match error {
            ConnectionHandlerUpgrErr::Timeout => self
                .pending_events
                .push_back(RpcHandlerEvent::InboundTimeout(info)),
            ConnectionHandlerUpgrErr::Upgrade(UpgradeError::Select(NegotiationError::Failed)) => {
                // The local peer merely doesn't support the protocol(s) requested.
                // This is no reason to close the connection, which may
                // successfully communicate with other protocols already.
                // An event is reported to permit user code to react to the fact that
                // the local peer does not support the requested protocol(s).
                self.pending_events
                    .push_back(RpcHandlerEvent::InboundUnsupportedProtocols(info));
            }
            _ => {
                // Anything else is considered a fatal error or misbehaviour of
                // the remote peer and results in closing the connection.
                self.pending_error = Some(error);
            }
        }
    }

    fn connection_keep_alive(&self) -> KeepAlive {
        self.keep_alive
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ConnectionHandlerEvent<RequestProtocol, RequestId, Self::OutEvent, Self::Error>> {
        // Check for a pending (fatal) error.
        if let Some(err) = self.pending_error.take() {
            // The handler will not be polled again by the `Swarm`.
            return Poll::Ready(ConnectionHandlerEvent::Close(err));
        }

        // Drain pending events.
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::Custom(event));
        } else if self.pending_events.capacity() > EMPTY_QUEUE_SHRINK_THRESHOLD {
            self.pending_events.shrink_to_fit();
        }

        // Check for inbound requests.
        while let Poll::Ready(Some(result)) = self.inbound.poll_next_unpin(cx) {
            match result {
                Ok(((id, rq), rs_sender)) => {
                    // We received an inbound request.
                    self.keep_alive = KeepAlive::Yes;
                    return Poll::Ready(ConnectionHandlerEvent::Custom(RpcHandlerEvent::Request {
                        request_id: id,
                        request: rq,
                        channel: rs_sender,
                    }));
                }
                Err(_err) => {
                    // The inbound upgrade has errored or timed out reading
                    // or waiting for the request. The handler is informed
                    // via `inject_listen_upgrade_error`.
                }
            }
        }

        // Emit outbound requests.
        if let Some(request) = self.outbound.pop_front() {
            let info = request.request_id;
            return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                protocol: SubstreamProtocol::new(request, info)
                    .with_timeout(self.substream_timeout),
            });
        }

        debug_assert!(self.outbound.is_empty());

        if self.outbound.capacity() > EMPTY_QUEUE_SHRINK_THRESHOLD {
            self.outbound.shrink_to_fit();
        }

        if self.inbound.is_empty() && self.keep_alive.is_yes() {
            // No new inbound or outbound requests. However, we may just have
            // started the latest inbound or outbound upgrade(s), so make sure
            // the keep-alive timeout is preceded by the substream timeout.
            let until = Instant::now() + self.substream_timeout + self.keep_alive_timeout;
            self.keep_alive = KeepAlive::Until(until);
        }

        Poll::Pending
    }
}
