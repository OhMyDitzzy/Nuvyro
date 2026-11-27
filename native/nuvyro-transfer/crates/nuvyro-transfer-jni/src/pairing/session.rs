use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nuvyro_transfer_core::protocol::{ErrorCode, Id, TLS_ALPN};
use rcgen::{
    CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose,
    PublicKeyData,
};
use ring::digest::{SHA256, digest};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::sign::{CertifiedKey, SingleCertAndKey};
use rustls::{ServerConfig, ServerConnection};
use zeroize::Zeroizing;

use super::{HandshakeGate, InvitationStore, SensitiveQr, random_id};

const LIFETIME: Duration = Duration::from_secs(86_400);
const CLOCK_ALLOWANCE: Duration = Duration::from_secs(300);

/// Session-local identity and its invitations; no key or ticket is persisted.
pub(crate) struct ReceiverIdentity {
    session: Id,
    pin: [u8; 32],
    config: Arc<ServerConfig>,
    issued_at: Instant,
    expires_at: Instant,
    not_before: SystemTime,
    not_after: SystemTime,
    invitations: Arc<InvitationStore>,
}

impl ReceiverIdentity {
    pub(crate) fn create() -> Result<Self, ErrorCode> {
        Self::at(SystemTime::now(), Instant::now())
    }

    fn at(now: SystemTime, issued_at: Instant) -> Result<Self, ErrorCode> {
        let seconds = now
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ErrorCode::SessionExpired)?
            .as_secs();
        let wall = rcgen::date_time_ymd(1970, 1, 1)
            .checked_add(
                Duration::from_secs(seconds)
                    .try_into()
                    .map_err(|_| ErrorCode::SessionExpired)?,
            )
            .ok_or(ErrorCode::SessionExpired)?;
        let not_before = wall
            .checked_sub(
                CLOCK_ALLOWANCE
                    .try_into()
                    .map_err(|_| ErrorCode::SessionExpired)?,
            )
            .ok_or(ErrorCode::SessionExpired)?;
        let not_after = wall
            .checked_add(LIFETIME.try_into().map_err(|_| ErrorCode::SessionExpired)?)
            .ok_or(ErrorCode::SessionExpired)?;
        // Use the same whole-second bounds as the DER certificate.
        let wall_now = UNIX_EPOCH
            .checked_add(Duration::from_secs(seconds))
            .ok_or(ErrorCode::SessionExpired)?;
        let mut params = CertificateParams::new(vec!["nuvyro.local".to_owned()])
            .map_err(|_| ErrorCode::InternalFailure)?;
        params
            .distinguished_name
            .push(DnType::CommonName, "nuvyro.local");
        params.not_before = not_before;
        params.not_after = not_after;
        params.is_ca = IsCa::ExplicitNoCa;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let key = Zeroizing::new(
            KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
                .map_err(|_| ErrorCode::InternalFailure)?,
        );
        let certificate = params
            .self_signed(&*key)
            .map_err(|_| ErrorCode::InternalFailure)?
            .der()
            .clone();
        if certificate.as_ref().len() > 4096 {
            return Err(ErrorCode::InternalFailure);
        }
        let pin: [u8; 32] = digest(&SHA256, &key.subject_public_key_info())
            .as_ref()
            .try_into()
            .map_err(|_| ErrorCode::InternalFailure)?;
        if pin == [0; 32] {
            return Err(ErrorCode::InternalFailure);
        }
        let private = Zeroizing::new(PrivateKeyDer::from(PrivatePkcs8KeyDer::from(
            key.serialize_der(),
        )));
        // Borrow the temporary DER so it can be cleared without another owned DER copy.
        let signing_key = rustls::crypto::ring::sign::any_supported_type(&private)
            .map_err(|_| ErrorCode::InternalFailure)?;
        let certified_key = CertifiedKey::new(vec![certificate], signing_key);
        certified_key
            .keys_match()
            .map_err(|_| ErrorCode::InternalFailure)?;
        let mut config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])
                .map_err(|_| ErrorCode::InternalFailure)?
                .with_no_client_auth()
                .with_cert_resolver(Arc::new(SingleCertAndKey::from(certified_key)));
        config.alpn_protocols = vec![TLS_ALPN.to_vec()];
        config.send_tls13_tickets = 0;
        config.max_tls13_tickets = 0;
        config.session_storage = Arc::new(rustls::server::NoServerSessionStorage {});
        config.max_early_data_size = 0;
        config.send_half_rtt_data = false;
        config.key_log = Arc::new(rustls::NoKeyLog);
        config.enable_secret_extraction = false;
        let session = random_id()?;
        Ok(Self {
            session,
            pin,
            config: Arc::new(config),
            issued_at,
            expires_at: issued_at
                .checked_add(LIFETIME)
                .ok_or(ErrorCode::SessionExpired)?,
            not_before: wall_now
                .checked_sub(CLOCK_ALLOWANCE)
                .ok_or(ErrorCode::SessionExpired)?,
            not_after: wall_now
                .checked_add(LIFETIME)
                .ok_or(ErrorCode::SessionExpired)?,
            invitations: Arc::new(InvitationStore::new(session, pin)),
        })
    }

    pub(crate) fn session(&self) -> Id {
        self.session
    }
    pub(crate) fn pin(&self) -> &[u8; 32] {
        &self.pin
    }

    pub(crate) fn check_live(&self) -> Result<(), ErrorCode> {
        self.check_live_at(SystemTime::now(), Instant::now())
    }

    pub(crate) fn check_live_at(&self, wall: SystemTime, now: Instant) -> Result<(), ErrorCode> {
        self.validate_at(wall, now)
    }

    pub(crate) fn new_server(&self) -> Result<ServerConnection, ErrorCode> {
        self.check_live()?;
        ServerConnection::new(Arc::clone(&self.config)).map_err(|_| ErrorCode::InternalFailure)
    }

    pub(crate) fn create_invitation(&self) -> Result<(Id, SensitiveQr), ErrorCode> {
        let now = Instant::now();
        self.validate_at(SystemTime::now(), now)?;
        self.invitations
            .issue(now)
            .map_err(|error| error.remote_code())
    }

    /// Construct only after this session's server TLS profile has been verified by its owner.
    pub(crate) fn incoming_gate(&self) -> Result<HandshakeGate, ErrorCode> {
        let now = Instant::now();
        self.validate_at(SystemTime::now(), now)?;
        HandshakeGate::incoming(Arc::clone(&self.invitations), now)
    }

    pub(crate) fn close(&self) {
        self.invitations.close();
    }

    fn validate_at(&self, wall: SystemTime, now: Instant) -> Result<(), ErrorCode> {
        if !self.invitations.is_open()
            || now < self.issued_at
            || now >= self.expires_at
            || wall < self.not_before
            || wall > self.not_after
        {
            self.close();
            return Err(ErrorCode::SessionExpired);
        }
        Ok(())
    }
}

impl Drop for ReceiverIdentity {
    fn drop(&mut self) {
        self.close();
    }
}

impl std::fmt::Debug for ReceiverIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReceiverIdentity([REDACTED])")
    }
}

#[cfg(test)]
mod tests;
