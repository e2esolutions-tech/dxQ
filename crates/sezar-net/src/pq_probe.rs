//! PQ-capable TLS probe.
//!
//! Opens a single TLS 1.3 handshake against a host, advertising the
//! `X25519MLKEM768` hybrid post-quantum key-exchange group alongside
//! the classical groups. Captures:
//!
//! - negotiated protocol version
//! - negotiated cipher suite
//! - **negotiated key-exchange group** — the load-bearing signal for
//!   PQ adoption; if the server picks `X25519MLKEM768`, it is
//!   demonstrably PQ-capable
//! - leaf certificate signature algorithm
//!
//! Implementation: rustls 0.23 with `rustls-post-quantum`'s
//! `provider()` (which registers ML-KEM-based hybrid groups on top of
//! the default ring crypto provider). Certificate verification is
//! disabled — we observe the chain, we do not authenticate against
//! it. The probe is intended for *observability*, not authenticated
//! transport.
//!
//! Ethics: one TCP connection per host, 5-second timeouts, single
//! handshake. Same envelope as the Python ssl probe in
//! `studies/study1/probe.py`.

use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tracing::debug;
use x509_parser::prelude::FromDer;

/// Single-host probe result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PqProbeResult {
    /// Host probed.
    pub host: String,
    /// Whether the handshake completed.
    pub ok: bool,
    /// Negotiated TLS version (`TLSv1_3` etc.). `None` on failure.
    pub protocol_version: Option<String>,
    /// Negotiated cipher suite IANA name. `None` on failure.
    pub cipher_suite: Option<String>,
    /// Negotiated key-exchange group, IANA name (e.g.
    /// `X25519MLKEM768`, `X25519`, `secp256r1`). The headline PQ
    /// indicator.
    pub kex_group: Option<String>,
    /// Whether the negotiated kex group is post-quantum.
    pub kex_pq: bool,
    /// Leaf certificate signature algorithm if parseable.
    pub cert_sig_algo: Option<String>,
    /// Leaf certificate subject CN (informational).
    pub cert_subject: Option<String>,
    /// Free-form error string when `ok` is false.
    pub error: Option<String>,
}

impl PqProbeResult {
    fn fail(host: &str, e: impl ToString) -> Self {
        Self {
            host: host.into(),
            ok: false,
            protocol_version: None,
            cipher_suite: None,
            kex_group: None,
            kex_pq: false,
            cert_sig_algo: None,
            cert_subject: None,
            error: Some(e.to_string()),
        }
    }
}

/// Cert verifier that accepts every chain — we are *observing*,
/// not authenticating.
#[derive(Debug)]
struct NoopVerifier;

impl ServerCertVerifier for NoopVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::RSA_PKCS1_SHA1,
        ]
    }
}

/// Build a rustls `ClientConfig` whose crypto provider includes
/// the `X25519MLKEM768` hybrid post-quantum kex group.
fn pq_client_config() -> Arc<ClientConfig> {
    let provider = rustls_post_quantum::provider();
    let cfg = ClientConfig::builder_with_provider(Arc::new(provider))
        .with_safe_default_protocol_versions()
        .expect("rustls protocol versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoopVerifier))
        .with_no_client_auth();
    Arc::new(cfg)
}

/// Map rustls's NamedGroup numeric ID to its IANA spelling. Subset
/// matching `sezar_net::algos::primitive_from_supported_group`'s
/// recognised vocabulary.
fn named_group_name(named_group_u16: u16) -> &'static str {
    match named_group_u16 {
        0x0017 => "secp256r1",
        0x0018 => "secp384r1",
        0x0019 => "secp521r1",
        0x001d => "x25519",
        0x001e => "x448",
        0x0100 => "ffdhe2048",
        0x0101 => "ffdhe3072",
        0x0102 => "ffdhe4096",
        0x11ec => "X25519MLKEM768",
        0x11eb => "SecP256r1MLKEM768",
        0x11ed => "P384MLKEM1024",
        0x0768 => "MLKEM768",
        0x0769 => "MLKEM512",
        0x076a => "MLKEM1024",
        _ => "unknown",
    }
}

/// Whether the named group is post-quantum (pure or hybrid).
fn is_pq_named_group(name: &str) -> bool {
    matches!(
        name,
        "X25519MLKEM768"
            | "SecP256r1MLKEM768"
            | "P384MLKEM1024"
            | "MLKEM768"
            | "MLKEM512"
            | "MLKEM1024"
    )
}

/// Parse a DER X.509 certificate and extract the signature algorithm
/// OID plus subject CN, both in human form.
fn parse_cert_metadata(der: &[u8]) -> (Option<String>, Option<String>) {
    let Ok((_, cert)) = x509_parser::certificate::X509Certificate::from_der(der) else {
        return (None, None);
    };
    let sig_algo = cert.signature_algorithm.algorithm.to_id_string();
    let sig_name = oid_to_sig_name(&sig_algo);
    let cn = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map(|s| s.to_string());
    (Some(sig_name), cn)
}

/// Translate a small set of well-known OIDs to canonical names; fall
/// back to the dotted OID itself for anything unrecognised.
fn oid_to_sig_name(oid: &str) -> String {
    match oid {
        // RSA + SHA-x (rsaEncryption arc)
        "1.2.840.113549.1.1.5" => "RSA-PKCS1-SHA1",
        "1.2.840.113549.1.1.11" => "RSA-PKCS1-SHA256",
        "1.2.840.113549.1.1.12" => "RSA-PKCS1-SHA384",
        "1.2.840.113549.1.1.13" => "RSA-PKCS1-SHA512",
        // RSA-PSS
        "1.2.840.113549.1.1.10" => "RSA-PSS",
        // ECDSA + SHA-x
        "1.2.840.10045.4.3.2" => "ECDSA-SHA256",
        "1.2.840.10045.4.3.3" => "ECDSA-SHA384",
        "1.2.840.10045.4.3.4" => "ECDSA-SHA512",
        // EdDSA
        "1.3.101.112" => "Ed25519",
        "1.3.101.113" => "Ed448",
        // ML-DSA (FIPS 204) — provisional OIDs
        "2.16.840.1.101.3.4.3.17" => "ML-DSA-44",
        "2.16.840.1.101.3.4.3.18" => "ML-DSA-65",
        "2.16.840.1.101.3.4.3.19" => "ML-DSA-87",
        // SLH-DSA (FIPS 205) — provisional OIDs
        "2.16.840.1.101.3.4.3.20" => "SLH-DSA-SHA2-128s",
        _ => return oid.to_string(),
    }
    .to_string()
}

/// Probe one host. `port = 443` is typical.
pub async fn probe(host: &str, port: u16, timeout: Duration) -> PqProbeResult {
    let config = pq_client_config();
    let connector = TlsConnector::from(config);

    let addr = format!("{host}:{port}");
    let tcp = match tokio::time::timeout(timeout, TcpStream::connect(&addr)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return PqProbeResult::fail(host, format!("connect: {e}")),
        Err(_) => return PqProbeResult::fail(host, "connect timeout"),
    };
    let server_name: ServerName<'static> = match ServerName::try_from(host.to_string()) {
        Ok(s) => s,
        Err(e) => return PqProbeResult::fail(host, format!("bad server name: {e}")),
    };
    let tls = match tokio::time::timeout(timeout, connector.connect(server_name, tcp)).await {
        Ok(Ok(t)) => t,
        Ok(Err(e)) => return PqProbeResult::fail(host, format!("tls handshake: {e}")),
        Err(_) => return PqProbeResult::fail(host, "handshake timeout"),
    };
    let (_, conn) = tls.get_ref();
    let protocol_version = conn.protocol_version().map(|v| format!("{:?}", v));
    let cipher_suite = conn
        .negotiated_cipher_suite()
        .map(|c| format!("{:?}", c.suite()));
    let kex_name = conn
        .negotiated_key_exchange_group()
        .map(|g| named_group_name(u16::from(g.name())).to_string());
    let kex_pq = kex_name.as_deref().map(is_pq_named_group).unwrap_or(false);
    let (cert_sig, cert_subject) = conn
        .peer_certificates()
        .and_then(|chain| chain.first())
        .map(|leaf| parse_cert_metadata(leaf.as_ref()))
        .unwrap_or((None, None));
    debug!(
        host,
        ?protocol_version,
        ?cipher_suite,
        ?kex_name,
        kex_pq,
        "probe complete"
    );
    // Close gracefully — we have what we need.
    let mut tls = tls;
    let _ = tls.shutdown().await;
    PqProbeResult {
        host: host.into(),
        ok: true,
        protocol_version,
        cipher_suite,
        kex_group: kex_name,
        kex_pq,
        cert_sig_algo: cert_sig,
        cert_subject,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classical_named_group_is_not_pq() {
        assert!(!is_pq_named_group("x25519"));
        assert!(!is_pq_named_group("secp256r1"));
    }

    #[test]
    fn hybrid_ml_kem_is_pq() {
        assert!(is_pq_named_group("X25519MLKEM768"));
        assert!(is_pq_named_group("SecP256r1MLKEM768"));
        assert!(is_pq_named_group("MLKEM768"));
    }

    #[test]
    fn named_group_table_covers_iana_codepoints() {
        assert_eq!(named_group_name(0x001d), "x25519");
        assert_eq!(named_group_name(0x0017), "secp256r1");
        assert_eq!(named_group_name(0x11ec), "X25519MLKEM768");
        // Unknown codepoints fall through to a stable marker rather than panicking.
        assert_eq!(named_group_name(0xffff), "unknown");
    }

    #[test]
    fn oid_to_sig_name_round_trips_known_cases() {
        assert_eq!(oid_to_sig_name("1.2.840.10045.4.3.2"), "ECDSA-SHA256");
        assert_eq!(oid_to_sig_name("1.2.840.113549.1.1.11"), "RSA-PKCS1-SHA256");
        assert_eq!(oid_to_sig_name("1.3.101.112"), "Ed25519");
        // Unknown OIDs round-trip as themselves so the operator can see them.
        assert_eq!(oid_to_sig_name("9.9.9.9"), "9.9.9.9");
    }
}
