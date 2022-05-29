use std::{sync::Arc, time::Duration};

use quinn::{IdleTimeout, TransportConfig};

pub(crate) fn insecure_server_config() -> quinn::ServerConfig {
    let cert = rcgen::generate_simple_self_signed(vec!["insecure".into()]).unwrap();
    let priv_key = rustls::PrivateKey(cert.serialize_private_key_der());
    let cert_der = rustls::Certificate(cert.serialize_der().unwrap());

    let mut server_config =
        quinn::ServerConfig::with_single_cert(vec![cert_der], priv_key).unwrap();

    let mut transport_config = TransportConfig::default();
    transport_config.keep_alive_interval(None);
    transport_config.max_idle_timeout(Some(
        IdleTimeout::try_from(Duration::from_millis(10_000)).unwrap(),
    ));
    server_config.transport = Arc::new(transport_config);

    server_config
}

pub(crate) fn insecure_client_config() -> quinn::ClientConfig {
    let crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(SkipServerVerification::new())
        .with_no_client_auth();

    let mut client_config = quinn::ClientConfig::new(Arc::new(crypto));

    let mut transport_config = TransportConfig::default();
    transport_config.keep_alive_interval(Some(Duration::from_millis(3_000)));
    transport_config.max_idle_timeout(Some(
        IdleTimeout::try_from(Duration::from_millis(10_000)).unwrap(),
    ));
    client_config.transport = Arc::new(transport_config);

    client_config
}

pub(crate) fn endpoint_config() -> quinn::EndpointConfig {
    quinn::EndpointConfig::default()
}

pub(crate) struct SkipServerVerification;

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}
