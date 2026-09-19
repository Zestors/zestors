use crate::NodeId;
use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::rustls::{
    self, ClientConfig, DigitallySignedStruct, RootCertStore, ServerConfig, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerifier},
    crypto::{CryptoProvider, ring},
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime, pem::PemObject},
    server::WebPkiClientVerifier,
};
use std::{io, sync::Arc};

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
}

/// The TLS identity of a cluster node.
///
/// With [`Tls::from_pem`] every node presents a certificate signed by the
/// cluster's CA and verifies the certificates of its peers (mutual TLS).
/// A peer is dialed using its node name as the server name, so a node's
/// certificate must carry its [`NodeId`](crate::NodeId) as a DNS subject
/// alternative name.
///
/// Mutual TLS authenticates *cluster membership*: every node holding a
/// certificate from the CA is trusted.
#[derive(Clone)]
pub struct Tls {
    server: Arc<ServerConfig>,
    client: Arc<ClientConfig>,
    /// Whether peers must prove, with their certificate, the node name they claim.
    verify_names: bool,
}

impl Tls {
    /// Builds a mutual-TLS configuration.
    ///
    /// - `ca_pem`: the certificate(s) of the cluster CA that peers must chain to.
    /// - `cert_pem`: this node's certificate chain, with its node name as SAN.
    /// - `key_pem`: this node's private key.
    pub fn from_pem(ca_pem: &[u8], cert_pem: &[u8], key_pem: &[u8]) -> Result<Self, TlsError> {
        let provider = provider();

        let mut roots = RootCertStore::empty();
        for ca in CertificateDer::pem_slice_iter(ca_pem) {
            roots.add(ca?)?;
        }
        let roots = Arc::new(roots);

        let chain = CertificateDer::pem_slice_iter(cert_pem).collect::<Result<Vec<_>, _>>()?;
        if chain.is_empty() {
            return Err(TlsError::NoCertificates);
        }
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
            server: Arc::new(server),
            client: Arc::new(client),
            verify_names: true,
        })
    }

    /// A self-signed configuration that performs **no verification**: every
    /// peer is accepted and nothing is authenticated. Traffic is encrypted, but
    /// anyone who can reach the port can join the cluster.
    ///
    /// Only meant for local development and examples.
    pub fn insecure_dev() -> Result<Self, TlsError> {
        tracing::warn!(
            "Using insecure TLS: peers are not verified and anyone can join this cluster"
        );
        let provider = provider();

        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
        let chain = vec![cert.der().clone()];
        let key = PrivateKeyDer::try_from(signing_key.serialize_der())
            .expect("rcgen produces valid PKCS#8");

        let mut server = ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(chain, key)?;
        server.alpn_protocols = vec![ALPN.to_vec()];

        let mut client = ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAnything(provider)))
            .with_no_client_auth();
        client.alpn_protocols = vec![ALPN.to_vec()];

        Ok(Self {
            server: Arc::new(server),
            client: Arc::new(client),
            verify_names: false,
        })
    }
}

impl Tls {
    /// The QUIC server and client configurations that use this identity.
    pub(super) fn quic_configs(
        &self,
        transport: Arc<quinn::TransportConfig>,
    ) -> io::Result<(quinn::ServerConfig, quinn::ClientConfig)> {
        let invalid = |e| io::Error::new(io::ErrorKind::InvalidInput, e);

        let server_crypto = QuicServerConfig::try_from(self.server.clone()).map_err(invalid)?;
        let mut server = quinn::ServerConfig::with_crypto(Arc::new(server_crypto));
        server.transport_config(transport.clone());

        let client_crypto = QuicClientConfig::try_from(self.client.clone()).map_err(invalid)?;
        let mut client = quinn::ClientConfig::new(Arc::new(client_crypto));
        client.transport_config(transport);

        Ok((server, client))
    }

    /// Checks that the certificate `conn`'s peer presented is valid for the node
    /// name it claims. Mutual TLS only proves the peer holds *some* certificate
    /// from the cluster CA; without this any member could impersonate any other.
    /// Accepts everything if this identity doesn't verify peers.
    pub(super) fn verify_peer(
        &self,
        conn: &quinn::Connection,
        claimed: &NodeId,
    ) -> Result<(), BoxError> {
        if !self.verify_names {
            return Ok(());
        }
        let identity = conn
            .peer_identity()
            .ok_or("peer presented no certificate")?;
        let chain = identity
            .downcast::<Vec<CertificateDer<'static>>>()
            .map_err(|_| "unexpected peer identity type")?;
        let end_entity = chain.first().ok_or("empty certificate chain")?;
        let name = ServerName::try_from(claimed.as_str())?;
        webpki::EndEntityCert::try_from(end_entity)?.verify_is_valid_for_subject_name(&name)?;
        Ok(())
    }
}

impl std::fmt::Debug for Tls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tls").finish_non_exhaustive()
    }
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(ring::default_provider())
}

/// Accepts every server certificate. Development only, see [`Tls::insecure_dev`].
#[derive(Debug)]
struct AcceptAnything(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAnything {
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
