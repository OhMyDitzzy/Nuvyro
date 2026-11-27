use std::sync::Arc;
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use nuvyro_transfer_core::pairing::PairingQr;
use nuvyro_transfer_core::protocol::PAIRING_QR_PREFIX;
use rcgen::{CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose};
use ring::digest::{SHA256, digest};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, UnixTime};
use rustls::time_provider::TimeProvider;
use rustls::{ClientConfig, ClientConnection, ServerConfig, ServerConnection};
use x509_parser::prelude::{FromDer, X509Certificate};

use super::super::SERVER_NAME;
use super::super::client::client_config;

pub(super) const NOW_SECONDS: u64 = 1_795_737_600;

#[derive(Debug)]
pub(super) struct FixedTime;

impl TimeProvider for FixedTime {
    fn current_time(&self) -> Option<UnixTime> {
        Some(now())
    }
}

pub(super) fn now() -> UnixTime {
    at(NOW_SECONDS)
}

pub(super) fn at(seconds: u64) -> UnixTime {
    UnixTime::since_unix_epoch(Duration::from_secs(seconds))
}

pub(super) fn params() -> CertificateParams {
    let mut params = CertificateParams::new(vec![SERVER_NAME.to_owned()]).unwrap();
    params.serial_number = Some(1_u64.into());
    params
        .distinguished_name
        .push(DnType::CommonName, SERVER_NAME);
    let issued = rcgen::date_time_ymd(1970, 1, 1) + Duration::from_secs(NOW_SECONDS);
    params.not_before = issued - Duration::from_secs(300);
    params.not_after = issued + Duration::from_secs(86_400);
    params.is_ca = IsCa::ExplicitNoCa;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params
}

pub(super) struct Identity {
    pub(super) certificate: CertificateDer<'static>,
    pub(super) key: PrivateKeyDer<'static>,
    pub(super) pin: [u8; 32],
}

pub(super) fn identity(params: CertificateParams) -> Identity {
    let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
    identity_with_key(params, key)
}

pub(super) fn identity_with_key(params: CertificateParams, key: KeyPair) -> Identity {
    let certificate = params.self_signed(&key).unwrap().der().clone();
    let pin = spki_pin(certificate.as_ref());
    Identity {
        certificate,
        key: PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
        pin,
    }
}

pub(super) fn spki_pin(der: &[u8]) -> [u8; 32] {
    let (_, certificate) = X509Certificate::from_der(der).unwrap();
    digest(&SHA256, certificate.public_key().raw)
        .as_ref()
        .try_into()
        .unwrap()
}

pub(super) fn scan(pin: &[u8; 32]) -> PairingQr {
    let mut payload = [0; 104];
    payload[..4].copy_from_slice(&[0, 1, 0, 0]);
    payload[4..20].fill(0x11);
    payload[20..36].fill(0x22);
    payload[36..68].copy_from_slice(pin);
    payload[68..100].fill(0x44);
    payload[100..102].copy_from_slice(&47739_u16.to_be_bytes());
    payload[102..104].copy_from_slice(&120_u16.to_be_bytes());
    let mut encoded = [0; 140];
    let len = URL_SAFE_NO_PAD.encode_slice(payload, &mut encoded).unwrap();
    let mut text = PAIRING_QR_PREFIX.as_bytes().to_vec();
    text.extend_from_slice(&encoded[..len]);
    PairingQr::decode(&text).unwrap()
}

pub(super) fn client_configuration(pin: &[u8; 32]) -> Arc<ClientConfig> {
    let qr = scan(pin);
    let mut config = (*client_config(qr.spki_pin()).unwrap()).clone();
    config.time_provider = Arc::new(FixedTime);
    Arc::new(config)
}

pub(super) fn client(config: Arc<ClientConfig>) -> ClientConnection {
    ClientConnection::new(config, SERVER_NAME.try_into().unwrap()).unwrap()
}

pub(super) fn server_configuration(identity: &Identity, alpn: Vec<Vec<u8>>) -> ServerConfig {
    let mut config =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![identity.certificate.clone()], identity.key.clone_key())
            .unwrap();
    config.alpn_protocols = alpn;
    config
}

pub(super) fn handshake(
    client: &mut ClientConnection,
    server: &mut ServerConnection,
) -> Result<(), ()> {
    for _ in 0..64 {
        if client.wants_write() {
            let mut bytes = Vec::new();
            client.write_tls(&mut bytes).map_err(|_| ())?;
            server
                .read_tls(&mut std::io::Cursor::new(bytes))
                .map_err(|_| ())?;
            server.process_new_packets().map_err(|_| ())?;
        }
        if server.wants_write() {
            let mut bytes = Vec::new();
            server.write_tls(&mut bytes).map_err(|_| ())?;
            client
                .read_tls(&mut std::io::Cursor::new(bytes))
                .map_err(|_| ())?;
            client.process_new_packets().map_err(|_| ())?;
        }
        if !client.is_handshaking()
            && !server.is_handshaking()
            && !client.wants_write()
            && !server.wants_write()
        {
            return Ok(());
        }
    }
    Err(())
}
