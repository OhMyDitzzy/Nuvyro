use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use nuvyro_transfer_core::protocol::TLS_ALPN;
use rustls::pki_types::SubjectPublicKeyInfoDer;
use rustls::sign::{CertifiedKey, Signer, SigningKey, SingleCertAndKey};
use rustls::{
    Error, HandshakeKind, ProtocolVersion, ServerConnection, SignatureAlgorithm, SignatureScheme,
};

use super::super::client::{client_config, unix_time};
use super::super::{TlsPolicyError, check_negotiated_profile, new_client};
use super::fixtures::{
    client, client_configuration, handshake, identity, params, scan, server_configuration,
};

#[test]
fn a_full_in_memory_handshake_requires_pin_signature_tls13_and_alpn() {
    let identity = identity(params());
    let mut client = client(client_configuration(&identity.pin));
    let config = server_configuration(&identity, vec![TLS_ALPN.to_vec()]);
    let mut server = ServerConnection::new(Arc::new(config)).unwrap();
    assert_eq!(
        check_negotiated_profile(&client),
        Err(TlsPolicyError::HandshakeIncomplete)
    );
    assert_eq!(handshake(&mut client, &mut server), Ok(()));
    assert_eq!(client.protocol_version(), Some(ProtocolVersion::TLSv1_3));
    assert_eq!(client.alpn_protocol(), Some(TLS_ALPN));
    assert_eq!(check_negotiated_profile(&client), Ok(()));
}

#[test]
fn a_wrong_pin_aborts_the_handshake() {
    let identity = identity(params());
    let mut wrong = identity.pin;
    wrong[0] ^= 1;
    let mut client = client(client_configuration(&wrong));
    let mut server = ServerConnection::new(Arc::new(server_configuration(
        &identity,
        vec![TLS_ALPN.to_vec()],
    )))
    .unwrap();
    assert!(handshake(&mut client, &mut server).is_err());
    assert!(check_negotiated_profile(&client).is_err());
}

#[test]
fn a_bad_certificate_verify_signature_aborts_despite_a_valid_pinned_leaf() {
    let identity = identity(params());
    let mut client = client(client_configuration(&identity.pin));
    let actual = rustls::crypto::ring::sign::any_supported_type(&identity.key).unwrap();
    let signatures = Arc::new(AtomicUsize::new(0));
    let key = CertifiedKey::new(
        vec![identity.certificate.clone()],
        Arc::new(CorruptSigningKey {
            inner: actual,
            signatures: Arc::clone(&signatures),
        }),
    );
    let mut config = server_configuration(&identity, vec![TLS_ALPN.to_vec()]);
    config.cert_resolver = Arc::new(SingleCertAndKey::from(key));
    let mut server = ServerConnection::new(Arc::new(config)).unwrap();
    assert!(handshake(&mut client, &mut server).is_err());
    assert_eq!(signatures.load(Ordering::Relaxed), 1);
    assert!(check_negotiated_profile(&client).is_err());
}

#[test]
fn missing_alpn_is_rejected_after_an_otherwise_valid_tls_handshake() {
    let identity = identity(params());
    let mut client = client(client_configuration(&identity.pin));
    let mut server =
        ServerConnection::new(Arc::new(server_configuration(&identity, vec![]))).unwrap();
    assert_eq!(handshake(&mut client, &mut server), Ok(()));
    assert_eq!(
        check_negotiated_profile(&client),
        Err(TlsPolicyError::AlpnMismatch)
    );
}

#[test]
fn a_different_alpn_does_not_negotiate() {
    let identity = identity(params());
    let mut client = client(client_configuration(&identity.pin));
    let config = server_configuration(&identity, vec![b"other-protocol/1".to_vec()]);
    let mut server = ServerConnection::new(Arc::new(config)).unwrap();
    assert!(handshake(&mut client, &mut server).is_err());
}

#[test]
fn new_connections_do_not_resume_or_send_early_data_even_with_reused_configs() {
    let identity = identity(params());
    let clients = client_configuration(&identity.pin);
    let server_config = server_configuration(&identity, vec![TLS_ALPN.to_vec()]);
    assert!(server_config.send_tls13_tickets > 0);
    let servers = Arc::new(server_config);
    for _ in 0..2 {
        let mut client = client(Arc::clone(&clients));
        assert!(client.early_data().is_none());
        let mut server = ServerConnection::new(Arc::clone(&servers)).unwrap();
        assert_eq!(handshake(&mut client, &mut server), Ok(()));
        assert_eq!(client.handshake_kind(), Some(HandshakeKind::Full));
        assert_eq!(check_negotiated_profile(&client), Ok(()));
    }
}

#[test]
fn production_client_defaults_disable_secret_logging_and_early_data() {
    let identity = identity(params());
    let qr = scan(&identity.pin);
    let config = client_config(qr.spki_pin()).unwrap();
    assert_eq!(config.alpn_protocols, vec![TLS_ALPN.to_vec()]);
    assert!(!config.enable_early_data);
    assert!(!config.enable_secret_extraction);
    assert!(!config.key_log.will_log("CLIENT_RANDOM"));
    let client = new_client(qr.spki_pin()).unwrap();
    assert_eq!(
        check_negotiated_profile(&client),
        Err(TlsPolicyError::HandshakeIncomplete)
    );
}

#[test]
fn wall_clock_conversion_never_replaces_an_invalid_clock_with_the_epoch() {
    assert!(unix_time(UNIX_EPOCH - Duration::from_secs(1)).is_none());
    assert_eq!(unix_time(UNIX_EPOCH).unwrap().as_secs(), 0);
    assert_eq!(
        unix_time(UNIX_EPOCH + Duration::from_secs(123))
            .unwrap()
            .as_secs(),
        123
    );
}

struct CorruptSigningKey {
    inner: Arc<dyn SigningKey>,
    signatures: Arc<AtomicUsize>,
}

impl fmt::Debug for CorruptSigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CorruptSigningKey(test)")
    }
}

impl SigningKey for CorruptSigningKey {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        self.inner.choose_scheme(offered).map(|signer| {
            Box::new(CorruptSigner {
                inner: signer,
                signatures: Arc::clone(&self.signatures),
            }) as Box<dyn Signer>
        })
    }

    fn public_key(&self) -> Option<SubjectPublicKeyInfoDer<'_>> {
        self.inner.public_key()
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        self.inner.algorithm()
    }
}

struct CorruptSigner {
    inner: Box<dyn Signer>,
    signatures: Arc<AtomicUsize>,
}

impl fmt::Debug for CorruptSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CorruptSigner(test)")
    }
}

impl Signer for CorruptSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let mut signature = self.inner.sign(message)?;
        self.signatures.fetch_add(1, Ordering::Relaxed);
        let last = signature
            .last_mut()
            .ok_or_else(|| Error::General("Empty test signature".into()))?;
        *last ^= 1;
        Ok(signature)
    }

    fn scheme(&self) -> SignatureScheme {
        self.inner.scheme()
    }
}
