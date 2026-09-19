use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::rustls::{
    self, ClientConfig, DigitallySignedStruct, DistinguishedName, RootCertStore, ServerConfig,
    SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerifier},
    crypto::{CryptoProvider, ring},
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime, pem::PemObject},
    server::{
        WebPkiClientVerifier,
        danger::{ClientCertVerified, ClientCertVerifier},
    },
};
use std::{io, sync::Arc};
use zestors_distr_backend::NodeName;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// The ALPN protocol id all cluster connections use.
const ALPN: &[u8] = b"zestors/1";

/// Errors that can occur while building a [`Tls`] configuration.
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("Invalid PEM input: {0}")]
    Pem(#[from] rustls::pki_types::pem::Error),
    #[error("No certificates found in PEM input")]
    NoCertificates,
    #[error("Invalid TLS configuration: {0}")]
    Rustls(#[from] rustls::Error),
    #[error("Failed to build client certificate verifier: {0}")]
    Verifier(#[from] rustls::server::VerifierBuilderError),
    #[error("Failed to generate a self-signed certificate: {0}")]
    Generate(#[from] rcgen::Error),
    #[error("The certificate must carry exactly one DNS name, the node name; found {0}")]
    CertificateNames(usize),
    #[error("Invalid certificate: {0}")]
    Certificate(String),
}

/// The TLS identity of a cluster node.
///
/// With [`Tls::from_pem`] every node presents a certificate signed by the
/// cluster's CA and verifies the certificates of its peers (mutual TLS).
///
/// A certificate belongs to one node: it carries exactly one DNS name, the
/// node's [`NodeId`](zestors_distr_backend::NodeId). That name is how a node is dialed (as the
/// TLS server name), and who a peer is, as far as the cluster is concerned, is
/// the name in the certificate it presented.
///
/// Mutual TLS authenticates *cluster membership*: every node holding a
/// certificate from the CA is trusted.
#[derive(Clone)]
pub struct Tls {
    mode: Mode,
}

#[derive(Clone)]
enum Mode {
    /// A certificate from the cluster CA, for `node`.
    Ca {
        server: Arc<ServerConfig>,
        client: Arc<ClientConfig>,
        node: NodeName,
    },
    /// Nothing is verified. Each node makes a self-signed certificate for its
    /// own name when it starts.
    Insecure,
}

impl Tls {
    /// Builds a mutual-TLS configuration.
    ///
    /// - `ca_pem`: the certificate(s) of the cluster CA that peers must chain to.
    /// - `cert_pem`: this node's certificate chain. The first certificate must
    ///   carry exactly one DNS name, the node name.
    /// - `key_pem`: this node's private key.
    pub fn from_pem(ca_pem: &[u8], cert_pem: &[u8], key_pem: &[u8]) -> Result<Self, TlsError> {
        let provider = provider();

        let mut roots = RootCertStore::empty();
        for ca in CertificateDer::pem_slice_iter(ca_pem) {
            roots.add(ca?)?;
        }
        let roots = Arc::new(roots);

        let chain = CertificateDer::pem_slice_iter(cert_pem).collect::<Result<Vec<_>, _>>()?;
        let node = node_of(chain.first().ok_or(TlsError::NoCertificates)?)?;
        let key = PrivateKeyDer::from_pem_slice(key_pem)?;

        let client_verifier =
            WebPkiClientVerifier::builder_with_provider(roots.clone(), provider.clone()).build()?;

        let mut server = ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()?
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(chain.clone(), key.clone_key())?;
        server.alpn_protocols = vec![ALPN.to_vec()];

        let mut client = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots)
            .with_client_auth_cert(chain, key)?;
        client.alpn_protocols = vec![ALPN.to_vec()];

        Ok(Self {
            mode: Mode::Ca {
                server: Arc::new(server),
                client: Arc::new(client),
                node,
            },
        })
    }

    /// A configuration that performs **no verification**: every peer is
    /// accepted, and is whoever its self-signed certificate says. Traffic is
    /// encrypted, but anyone who can reach the port can join the cluster, as
    /// any node.
    ///
    /// Only meant for local development and examples.
    pub fn insecure_dev() -> Result<Self, TlsError> {
        tracing::warn!(
            "Using insecure TLS: peers are not verified and anyone can join this cluster"
        );
        Ok(Self {
            mode: Mode::Insecure,
        })
    }
}

impl Tls {
    /// The QUIC server and client configurations that make the node `local`
    /// present this identity. Fails if the certificate is for another node.
    pub(super) fn quic_configs(
        &self,
        local: &NodeName,
        transport: Arc<quinn::TransportConfig>,
    ) -> io::Result<(quinn::ServerConfig, quinn::ClientConfig)> {
        let (server, client) = match &self.mode {
            Mode::Ca {
                server,
                client,
                node,
            } => {
                if node != local {
                    return Err(invalid(format!(
                        "The certificate is for node {node}, not {local}"
                    )));
                }
                (server.clone(), client.clone())
            }
            Mode::Insecure => insecure_configs(local).map_err(invalid)?,
        };

        let server_crypto = QuicServerConfig::try_from(server).map_err(invalid)?;
        let mut server = quinn::ServerConfig::with_crypto(Arc::new(server_crypto));
        server.transport_config(transport.clone());

        let client_crypto = QuicClientConfig::try_from(client).map_err(invalid)?;
        let mut client = quinn::ClientConfig::new(Arc::new(client_crypto));
        client.transport_config(transport);

        Ok((server, client))
    }
}

fn invalid(error: impl Into<BoxError>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error)
}

/// The TLS configurations of an insecure node: a self-signed certificate for
/// `local`, and no verification of anyone else's.
fn insecure_configs(local: &NodeName) -> Result<(Arc<ServerConfig>, Arc<ClientConfig>), TlsError> {
    let provider = provider();

    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec![local.as_str().to_string()])?;
    let chain = vec![cert.der().clone()];
    let key =
        PrivateKeyDer::try_from(signing_key.serialize_der()).expect("rcgen produces valid PKCS#8");

    let mut server = ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?
        .with_client_cert_verifier(Arc::new(AcceptAnyone(provider.clone())))
        .with_single_cert(chain.clone(), key.clone_key())?;
    server.alpn_protocols = vec![ALPN.to_vec()];

    let mut client = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyone(provider)))
        .with_client_auth_cert(chain, key)?;
    client.alpn_protocols = vec![ALPN.to_vec()];

    Ok((Arc::new(server), Arc::new(client)))
}

/// The node a certificate belongs to: its one DNS name.
fn node_of(cert: &CertificateDer<'_>) -> Result<NodeName, TlsError> {
    let cert = webpki::EndEntityCert::try_from(cert)
        .map_err(|err| TlsError::Certificate(err.to_string()))?;
    // Only for reading the name: whether the certificate is valid for it has
    // been decided by the TLS handshake.
    let mut names = cert.valid_dns_names();
    match (names.next(), names.next()) {
        (Some(name), None) => Ok(NodeName::new(name)),
        (None, _) => Err(TlsError::CertificateNames(0)),
        (Some(_), Some(_)) => Err(TlsError::CertificateNames(2 + names.count())),
    }
}

/// The node on the other end of `conn`, from the certificate it presented in
/// the TLS handshake, which has verified it.
pub(super) fn peer_of(conn: &quinn::Connection) -> Result<NodeName, BoxError> {
    let identity = conn
        .peer_identity()
        .ok_or("peer presented no certificate")?;
    let chain = identity
        .downcast::<Vec<CertificateDer<'static>>>()
        .map_err(|_| "unexpected peer identity type")?;
    let end_entity = chain.first().ok_or("empty certificate chain")?;
    Ok(node_of(end_entity)?)
}

impl std::fmt::Debug for Tls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tls").finish_non_exhaustive()
    }
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(ring::default_provider())
}

/// Accepts every certificate. Development only, see [`Tls::insecure_dev`].
#[derive(Debug)]
struct AcceptAnyone(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAnyone {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

impl ClientCertVerifier for AcceptAnyone {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ca {
        cert: rcgen::Certificate,
        issuer: rcgen::Issuer<'static, rcgen::KeyPair>,
    }

    impl Ca {
        fn new() -> Self {
            let key = rcgen::KeyPair::generate().unwrap();
            let mut params = rcgen::CertificateParams::new(vec![]).unwrap();
            params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
            let cert = params.self_signed(&key).unwrap();
            let issuer = rcgen::Issuer::new(params, key);
            Self { cert, issuer }
        }

        /// A `Tls` for a certificate valid for all of `names`.
        fn tls(&self, names: &[&str]) -> Result<Tls, TlsError> {
            let key = rcgen::KeyPair::generate().unwrap();
            let params = rcgen::CertificateParams::new(
                names
                    .iter()
                    .map(|name| name.to_string())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            let cert = params.signed_by(&key, &self.issuer).unwrap();
            Tls::from_pem(
                self.cert.pem().as_bytes(),
                cert.pem().as_bytes(),
                key.serialize_pem().as_bytes(),
            )
        }
    }

    #[test]
    fn a_certificate_with_one_name_names_the_node() {
        let tls = Ca::new().tls(&["node-a"]).unwrap();
        let Mode::Ca { node, .. } = &tls.mode else {
            panic!("A CA identity")
        };
        assert_eq!(node.as_str(), "node-a");
    }

    #[test]
    fn a_certificate_must_have_exactly_one_name() {
        let ca = Ca::new();
        assert!(matches!(
            ca.tls(&["node-a", "node-b"]),
            Err(TlsError::CertificateNames(2))
        ));
        assert!(matches!(ca.tls(&[]), Err(TlsError::CertificateNames(0))));
    }

    #[test]
    fn a_node_can_only_use_a_certificate_for_its_own_name() {
        let tls = Ca::new().tls(&["node-a"]).unwrap();
        let transport = || Arc::new(quinn::TransportConfig::default());
        assert!(
            tls.quic_configs(&NodeName::new("node-a"), transport())
                .is_ok()
        );
        let Err(err) = tls.quic_configs(&NodeName::new("node-b"), transport()) else {
            panic!("A mismatch is refused")
        };
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn insecure_nodes_get_a_certificate_for_their_own_name() {
        let (server, _) = insecure_configs(&NodeName::new("node-a")).unwrap();
        drop(server);
        let tls = Tls::insecure_dev().unwrap();
        assert!(
            tls.quic_configs(
                &NodeName::new("node-a"),
                Arc::new(quinn::TransportConfig::default())
            )
            .is_ok()
        );
    }
}
