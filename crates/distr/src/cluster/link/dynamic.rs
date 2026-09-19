//! The backend traits with their types erased, so that the rest of the cluster
//! isn't generic over which backend it runs on. The boxing costs one
//! allocation per opened stream, accepted stream and received datagram.

use crate::{
    NodeAddr, NodeId,
    backend::{Backend, Connection, DatagramError, Endpoint, LocalNode, RecvStream, SendStream},
};
use bytes::Bytes;
use std::{future::Future, io, pin::Pin, sync::Arc, time::Duration};

pub(super) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub(super) trait DynConnection: Send + Sync {
    fn open_stream(&self) -> BoxFuture<'_, io::Result<(SendStream, RecvStream)>>;
    fn accept_stream(&self) -> BoxFuture<'_, io::Result<(SendStream, RecvStream)>>;
    fn send_datagram(&self, data: Bytes) -> Result<(), DatagramError>;
    fn recv_datagram(&self) -> BoxFuture<'_, io::Result<Bytes>>;
    fn peer(&self) -> &NodeId;
    fn is_closed(&self) -> bool;
    fn close(&self);
}

impl<C: Connection> DynConnection for C {
    fn open_stream(&self) -> BoxFuture<'_, io::Result<(SendStream, RecvStream)>> {
        Box::pin(Connection::open_stream(self))
    }

    fn accept_stream(&self) -> BoxFuture<'_, io::Result<(SendStream, RecvStream)>> {
        Box::pin(Connection::accept_stream(self))
    }

    fn send_datagram(&self, data: Bytes) -> Result<(), DatagramError> {
        Connection::send_datagram(self, data)
    }

    fn recv_datagram(&self) -> BoxFuture<'_, io::Result<Bytes>> {
        Box::pin(Connection::recv_datagram(self))
    }

    fn peer(&self) -> &NodeId {
        Connection::peer(self)
    }

    fn is_closed(&self) -> bool {
        Connection::is_closed(self)
    }

    fn close(&self) {
        Connection::close(self)
    }
}

pub(super) trait DynEndpoint: Send + Sync {
    fn local_addr(&self) -> io::Result<NodeAddr>;
    fn connect<'a>(
        &'a self,
        addr: &'a NodeAddr,
        node: &'a NodeId,
    ) -> BoxFuture<'a, io::Result<Arc<dyn DynConnection>>>;
    fn accept(&self) -> BoxFuture<'_, io::Result<Arc<dyn DynConnection>>>;
    fn close(&self, grace: Duration) -> BoxFuture<'_, ()>;
}

impl<E: Endpoint> DynEndpoint for E {
    fn local_addr(&self) -> io::Result<NodeAddr> {
        Endpoint::local_addr(self)
    }

    fn connect<'a>(
        &'a self,
        addr: &'a NodeAddr,
        node: &'a NodeId,
    ) -> BoxFuture<'a, io::Result<Arc<dyn DynConnection>>> {
        Box::pin(async move {
            let conn = Endpoint::connect(self, addr, node).await?;
            Ok(Arc::new(conn) as Arc<dyn DynConnection>)
        })
    }

    fn accept(&self) -> BoxFuture<'_, io::Result<Arc<dyn DynConnection>>> {
        Box::pin(async move {
            let conn = Endpoint::accept(self).await?;
            Ok(Arc::new(conn) as Arc<dyn DynConnection>)
        })
    }

    fn close(&self, grace: Duration) -> BoxFuture<'_, ()> {
        Box::pin(Endpoint::close(self, grace))
    }
}

/// A [`Backend`] with its type erased, so that a config doesn't have to be
/// generic over it.
pub(super) trait ErasedBackend: Send {
    fn start(
        self: Box<Self>,
        local: LocalNode,
    ) -> BoxFuture<'static, io::Result<Box<dyn DynEndpoint>>>;
}

impl<B: Backend> ErasedBackend for B {
    fn start(
        self: Box<Self>,
        local: LocalNode,
    ) -> BoxFuture<'static, io::Result<Box<dyn DynEndpoint>>> {
        Box::pin(async move {
            let endpoint = Backend::start(*self, local).await?;
            Ok(Box::new(endpoint) as Box<dyn DynEndpoint>)
        })
    }
}
