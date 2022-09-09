use quinn::{ClientConfig, ServerConfig, VarInt};
use std::{sync::Arc, time::Duration};

pub(crate) fn insecure_client() -> ClientConfig {
    ClientConfig::new(Arc::new(
        rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(SkipServerVerification::new())
            .with_no_client_auth(),
    ))
}

pub(crate) fn insecure_server() -> ServerConfig {
    let raw_cert = rcgen::generate_simple_self_signed(vec![INSECURE_DOMAIN.into()]).unwrap();

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

static INSECURE_DOMAIN: &'static str = "insecure";

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
