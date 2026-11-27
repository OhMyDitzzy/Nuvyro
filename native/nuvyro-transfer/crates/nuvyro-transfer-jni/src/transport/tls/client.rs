use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use nuvyro_transfer_core::pairing::SpkiPin;
use nuvyro_transfer_core::protocol::TLS_ALPN;
use rustls::client::Resumption;
use rustls::pki_types::{ServerName, UnixTime};
use rustls::time_provider::TimeProvider;
use rustls::{ClientConfig, ClientConnection, ProtocolVersion};

use super::verifier::PinnedServerVerifier;
use super::{SERVER_NAME, TlsPolicyError};

pub(crate) fn new_client(pin: &SpkiPin) -> Result<ClientConnection, TlsPolicyError> {
    let name =
        ServerName::try_from(SERVER_NAME).map_err(|_| TlsPolicyError::InvalidConfiguration)?;
    ClientConnection::new(client_config(pin)?, name)
        .map_err(|_| TlsPolicyError::InvalidConfiguration)
}

pub(super) fn client_config(pin: &SpkiPin) -> Result<Arc<ClientConfig>, TlsPolicyError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(PinnedServerVerifier::new(pin, Arc::clone(&provider)));
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|_| TlsPolicyError::InvalidConfiguration)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    config.alpn_protocols = vec![TLS_ALPN.to_vec()];
    config.resumption = Resumption::disabled();
    config.enable_early_data = false;
    config.enable_secret_extraction = false;
    config.key_log = Arc::new(rustls::NoKeyLog);
    config.time_provider = Arc::new(WallClock);
    Ok(Arc::new(config))
}

/// Check before queuing any application bytes; abort on every TLS or I/O error.
/// TLS profile only. Session hello, invitation authentication, and consent still follow.
pub(crate) fn check_negotiated_profile(client: &ClientConnection) -> Result<(), TlsPolicyError> {
    if client.is_handshaking() {
        return Err(TlsPolicyError::HandshakeIncomplete);
    }
    if client.protocol_version() != Some(ProtocolVersion::TLSv1_3) {
        return Err(TlsPolicyError::ProtocolMismatch);
    }
    if client.alpn_protocol() != Some(TLS_ALPN) {
        return Err(TlsPolicyError::AlpnMismatch);
    }
    if client
        .peer_certificates()
        .is_none_or(|chain| chain.len() != 1)
    {
        return Err(TlsPolicyError::CertificateChain);
    }
    Ok(())
}

#[derive(Debug)]
struct WallClock;

impl TimeProvider for WallClock {
    fn current_time(&self) -> Option<UnixTime> {
        unix_time(SystemTime::now())
    }
}

pub(super) fn unix_time(time: SystemTime) -> Option<UnixTime> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(UnixTime::since_unix_epoch)
}
