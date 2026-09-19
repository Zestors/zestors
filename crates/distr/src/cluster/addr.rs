use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::{fmt, net::SocketAddr};

/// Where a node can be reached, in terms its [backend](crate::backend) understands.
///
/// For the QUIC backend this is `host:port`: a socket address, or a host name
/// that is resolved when connecting. Other backends define their own: a URL, a
/// key, a name. The cluster only carries it around, which is why it is opaque.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Addr(SmolStr);

impl Addr {
    pub fn new(addr: impl Into<SmolStr>) -> Self {
        Self(addr.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The address as a socket address, if it is one.
    pub fn to_socket_addr(&self) -> Option<SocketAddr> {
        self.0.parse().ok()
    }
}

impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<SocketAddr> for Addr {
    fn from(addr: SocketAddr) -> Self {
        Self(addr.to_string().into())
    }
}

impl From<&str> for Addr {
    fn from(addr: &str) -> Self {
        Self::new(addr)
    }
}

impl From<String> for Addr {
    fn from(addr: String) -> Self {
        Self::new(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_addresses_round_trip() {
        let socket: SocketAddr = "10.0.0.1:7000".parse().unwrap();
        assert_eq!(Addr::from(socket).to_socket_addr(), Some(socket));
        assert_eq!(Addr::new("db.internal:7000").to_socket_addr(), None);
    }
}
