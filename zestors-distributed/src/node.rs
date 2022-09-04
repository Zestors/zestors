use futures::StreamExt;
use quinn::{Connection, Endpoint, Incoming, IncomingBiStreams, IncomingUniStreams};
use std::{error::Error, io, net::SocketAddr};
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct System {
    endpoint: Endpoint,
}

pub struct InnerSystem {}

struct SystemActor {
    incoming: Incoming,
    system: System,
}

struct NodeActor {
    bi_streams: IncomingBiStreams,
    uni_streams: IncomingUniStreams,
}

#[derive(Clone)]
pub struct Node {
    conn: Connection,
}

impl SystemActor {
    fn new(addr: SocketAddr) -> io::Result<Self> {
        let (endpoint, incoming) = Endpoint::server(insecure::server_cfg(), addr)?;
        Ok(SystemActor {
            system: System { endpoint },
            incoming,
        })
    }

    fn system(&self) -> &System {
        &self.system
    }

    fn spawn(mut self) -> JoinHandle<NodeExit> {
        tokio::spawn(async move {
            loop {
                match self.incoming.next().await {
                    Some(connecting) => match connecting.await {
                        Ok(new_conn) => {
                            let conn = new_conn.connection;
                            let bi_streams = new_conn.bi_streams;
                            let uni_streams = new_conn.uni_streams;

                            todo!()
                        }
                        Err(e) => todo!(),
                    },
                    None => break NodeExit::Empty,
                }
            }
        })
    }
}

pub enum NodeExit {
    Empty,
}

pub async fn main() -> Result<(), Box<dyn Error>> {
    let addr = "127.0.0.1:5000".parse().unwrap();

    tokio::spawn(async move {
        let cfg = insecure::server_cfg();
        let (endpoint, mut incoming) = Endpoint::server(cfg, addr).unwrap();

        // accept a single connection
        let incoming_conn = incoming.next().await.unwrap();
        let new_conn = incoming_conn.await.unwrap();
        println!(
            "[server] connection accepted: addr={}",
            new_conn.connection.remote_address()
        );
    });

    let client_cfg = insecure::client_cfg();
    let mut endpoint = Endpoint::client("127.0.0.1:0".parse().unwrap())?;
    endpoint.set_default_client_config(client_cfg);

    // connect to server
    let quinn::NewConnection { connection, .. } =
        endpoint.connect(addr, "localhost").unwrap().await.unwrap();
    println!("[client] connected: addr={}", connection.remote_address());
    // Dropping handles allows the corresponding objects to automatically shut down
    drop(connection);
    // Make sure the server has a chance to clean up
    endpoint.wait_idle().await;

    Ok(())
}

mod insecure {
    use quinn::{ClientConfig, IdleTimeout, ServerConfig, VarInt};
    use std::{borrow::BorrowMut, error::Error, sync::Arc, time::Duration};

    pub(super) fn client_cfg() -> ClientConfig {
        ClientConfig::new(Arc::new(
            rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_custom_certificate_verifier(SkipServerVerification::new())
                .with_no_client_auth(),
        ))
    }

    pub(super) fn server_cfg() -> ServerConfig {
        let raw_cert = rcgen::generate_simple_self_signed(vec![DOMAIN.into()]).unwrap();

        let priv_key = rustls::PrivateKey(raw_cert.serialize_private_key_der());
        let cert = rustls::Certificate(raw_cert.serialize_der().unwrap());

        let mut cfg = ServerConfig::with_crypto(Arc::new(
            rustls::ServerConfig::builder()
                .with_safe_defaults()
                .with_no_client_auth()
                .with_single_cert(vec![cert], priv_key)
                .unwrap(),
        ));

        Arc::get_mut(&mut cfg.transport)
            .unwrap()
            .max_concurrent_bidi_streams(VarInt::MAX)
            .max_idle_timeout(Some(Duration::from_secs(100).try_into().unwrap()));

        cfg
    }

    struct SkipServerVerification;

    impl SkipServerVerification {
        fn new() -> Arc<Self> {
            Arc::new(Self)
        }
    }

    impl rustls::client::ServerCertVerifier for SkipServerVerification {
        fn verify_server_cert(
            &self,
            _: &rustls::Certificate,
            _: &[rustls::Certificate],
            _: &rustls::ServerName,
            _: &mut dyn Iterator<Item = &[u8]>,
            _: &[u8],
            _: std::time::SystemTime,
        ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::ServerCertVerified::assertion())
        }
    }

    static DOMAIN: &'static str = "insecure";
}
