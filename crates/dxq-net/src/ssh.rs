//! SSH key-exchange observation (SEZ-30).
//!
//! Passively reads the SSH2 identification banner and the server's
//! `SSH_MSG_KEXINIT` from a captured TCP stream, extracts the
//! `kex_algorithms` name-list, and classifies post-quantum hybrid
//! suites. No authentication is attempted and no payload past the
//! first KEXINIT is needed — everything observed here is sent in
//! the clear, before the SSH transport is encrypted.
//!
//! # What we parse
//!
//! 1. **Identification string** — `SSH-2.0-...\r\n`, the first line
//!    each side sends (RFC 4253 §4.2). We use it only to confirm the
//!    stream is SSH2 and to tag the software version.
//! 2. **`SSH_MSG_KEXINIT`** (byte 20, RFC 4253 §7.1) — the binary
//!    packet carrying ten algorithm name-lists. We read the first,
//!    `kex_algorithms`, which is where the PQ hybrids live.
//!
//! The binary packet framing is: `packet_length(4) padding_length(1)
//! payload(packet_length-padding_length-1) padding(...)`. The payload
//! begins with `msg_type(1)`; for KEXINIT it is `cookie(16)` then the
//! name-lists, each `length(4) + comma-separated ASCII`.
//!
//! # PQ classification
//!
//! OpenSSH ships two hybrid PQ key exchanges:
//! - `sntrup761x25519-sha512` (and the `@openssh.com` variant) —
//!   Streamlined NTRU Prime 761 + X25519.
//! - `mlkem768x25519-sha256` — ML-KEM-768 (FIPS 203) + X25519.
//!
//! The negotiated kex is the first algorithm the two offers share;
//! for a passive single-direction observation we report the first
//! PQ algorithm offered (preference order) if any, else the first
//! offered algorithm.

use dxq_core::{
    Asset, AssetKind, CryptoInventoryEvent, NistLevel, Posture, Primitive, PrimitiveRole,
    SCHEMA_MINOR, SCHEMA_VERSION,
};

const SSH_MSG_KEXINIT: u8 = 20;

/// Hybrid PQ kex algorithm names OpenSSH negotiates, mapped to a
/// canonical primitive name + NIST level. Matched case-sensitively
/// against the SSH name-list tokens (optionally with the
/// `@openssh.com` suffix stripped).
fn pq_kex_classification(name: &str) -> Option<(&'static str, NistLevel)> {
    let base = name.strip_suffix("@openssh.com").unwrap_or(name);
    match base {
        "sntrup761x25519-sha512" => Some(("sntrup761x25519", NistLevel::L5)),
        "mlkem768x25519-sha256" => Some(("mlkem768x25519", NistLevel::L3)),
        _ => None,
    }
}

/// Parsed summary of an observed SSH handshake.
#[derive(Debug, Clone, PartialEq)]
pub struct SshKexSummary {
    /// Peer software version from the identification banner, e.g.
    /// `"SSH-2.0-OpenSSH_9.6"`. `None` if the banner was not seen.
    pub banner: Option<String>,
    /// The `kex_algorithms` name-list in offered order.
    pub kex_algorithms: Vec<String>,
}

impl SshKexSummary {
    /// The algorithm we report as the session's key exchange: the
    /// first PQ-hybrid offered (by the sender's preference order),
    /// else the first offered algorithm.
    pub fn reported_kex(&self) -> Option<&str> {
        self.kex_algorithms
            .iter()
            .find(|a| pq_kex_classification(a).is_some())
            .or_else(|| self.kex_algorithms.first())
            .map(String::as_str)
    }

    /// Whether any offered kex is a recognised PQ hybrid.
    pub fn offers_pq(&self) -> bool {
        self.kex_algorithms
            .iter()
            .any(|a| pq_kex_classification(a).is_some())
    }

    /// Build the `kex` primitive for the reported algorithm.
    pub fn primitives(&self) -> Vec<Primitive> {
        let Some(kex) = self.reported_kex() else {
            return Vec::new();
        };
        match pq_kex_classification(kex) {
            Some((canon, level)) => vec![Primitive {
                role: PrimitiveRole::Kex,
                algorithm: canon.to_string(),
                parameters: Default::default(),
                pq_resistant: Some(true),
                nist_classification: Some(level),
            }],
            None => vec![Primitive {
                role: PrimitiveRole::Kex,
                algorithm: kex.to_string(),
                parameters: Default::default(),
                // Classical ECDH/DH kex: curve25519-sha256,
                // ecdh-sha2-*, diffie-hellman-* — not PQ.
                pq_resistant: Some(false),
                nist_classification: None,
            }],
        }
    }
}

/// Error cases when parsing an SSH stream slice.
#[derive(Debug, PartialEq)]
pub enum SshParseError {
    /// The byte slice is not an SSH2 identification banner and does
    /// not begin with a plausible binary packet.
    NotSsh,
    /// A binary packet header was found but it was truncated or the
    /// declared length is implausible for a single-segment capture.
    Truncated,
    /// A KEXINIT packet was located but its name-lists were malformed.
    Malformed,
}

/// Read a 4-byte big-endian length-prefixed, comma-separated ASCII
/// name-list starting at `buf[pos]`. Returns the tokens and the new
/// cursor position just past the list.
fn read_name_list(buf: &[u8], pos: usize) -> Result<(Vec<String>, usize), SshParseError> {
    if pos + 4 > buf.len() {
        return Err(SshParseError::Malformed);
    }
    let len = u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]) as usize;
    let start = pos + 4;
    let end = start.checked_add(len).ok_or(SshParseError::Malformed)?;
    if end > buf.len() {
        return Err(SshParseError::Malformed);
    }
    let s = std::str::from_utf8(&buf[start..end]).map_err(|_| SshParseError::Malformed)?;
    let tokens = if s.is_empty() {
        Vec::new()
    } else {
        s.split(',').map(str::to_string).collect()
    };
    Ok((tokens, end))
}

/// Parse an SSH stream slice (one direction of a TCP flow) for the
/// identification banner and the first KEXINIT. `buf` is the TCP
/// payload bytes in order; the banner (if present) precedes the
/// binary packet stream.
///
/// This is deliberately tolerant: a capture may contain only the
/// banner (KEXINIT in a later segment we dropped), only the KEXINIT
/// (banner in an earlier segment), or both.
pub fn parse_ssh(buf: &[u8]) -> Result<SshKexSummary, SshParseError> {
    let mut banner = None;
    let mut cursor = 0usize;

    // Optional leading identification line: "SSH-2.0-..." up to CRLF
    // (or LF). RFC 4253 allows pre-banner lines not starting with
    // "SSH-"; we skip any such lines until we see the SSH- line or
    // run into binary.
    while cursor < buf.len() && buf[cursor..].starts_with(b"SSH-") {
        let rest = &buf[cursor..];
        let line_end = rest
            .windows(2)
            .position(|w| w == b"\r\n")
            .map(|p| (p, 2))
            .or_else(|| rest.iter().position(|&b| b == b'\n').map(|p| (p, 1)));
        match line_end {
            Some((p, skip)) => {
                let line = String::from_utf8_lossy(&rest[..p]).trim().to_string();
                banner = Some(line);
                cursor += p + skip;
            }
            // Banner line not terminated in this slice — take what we
            // have and stop; there is no binary packet to read.
            None => {
                banner = Some(String::from_utf8_lossy(rest).trim().to_string());
                return Ok(SshKexSummary {
                    banner,
                    kex_algorithms: Vec::new(),
                });
            }
        }
    }

    // If we consumed only a banner and nothing else, return it.
    if cursor >= buf.len() {
        if banner.is_some() {
            return Ok(SshKexSummary {
                banner,
                kex_algorithms: Vec::new(),
            });
        }
        return Err(SshParseError::NotSsh);
    }

    // Now expect binary SSH packets. Scan forward for a KEXINIT.
    // Each packet: packet_length(4) padding_length(1) payload(..) pad.
    loop {
        if cursor + 5 > buf.len() {
            // No full packet header left. If we at least saw a
            // banner, that is still a useful (partial) observation.
            if banner.is_some() {
                return Ok(SshKexSummary {
                    banner,
                    kex_algorithms: Vec::new(),
                });
            }
            return Err(SshParseError::Truncated);
        }
        let pkt_len = u32::from_be_bytes([
            buf[cursor],
            buf[cursor + 1],
            buf[cursor + 2],
            buf[cursor + 3],
        ]) as usize;
        // Sanity: a KEXINIT is well under 64 KiB. Guard against
        // mis-synced bytes declaring an absurd length.
        if !(2..=65_535).contains(&pkt_len) {
            if banner.is_some() {
                return Ok(SshKexSummary {
                    banner,
                    kex_algorithms: Vec::new(),
                });
            }
            return Err(SshParseError::NotSsh);
        }
        let pad_len = buf[cursor + 4] as usize;
        let payload_start = cursor + 5;
        let payload_len = pkt_len.checked_sub(pad_len + 1).ok_or(SshParseError::Malformed)?;
        let payload_end = payload_start.checked_add(payload_len).ok_or(SshParseError::Malformed)?;
        if payload_end > buf.len() {
            // KEXINIT spilled past our single-segment slice.
            if banner.is_some() {
                return Ok(SshKexSummary {
                    banner,
                    kex_algorithms: Vec::new(),
                });
            }
            return Err(SshParseError::Truncated);
        }
        let payload = &buf[payload_start..payload_end];
        if !payload.is_empty() && payload[0] == SSH_MSG_KEXINIT {
            // payload: msg_type(1) cookie(16) then kex_algorithms.
            if payload.len() < 1 + 16 + 4 {
                return Err(SshParseError::Malformed);
            }
            let (kex_algorithms, _) = read_name_list(payload, 1 + 16)?;
            return Ok(SshKexSummary {
                banner,
                kex_algorithms,
            });
        }
        // Not KEXINIT — advance past this whole packet and keep
        // looking (e.g. a stray packet before KEXINIT).
        cursor = payload_end + pad_len;
    }
}

/// Build an `ssh_session` crypto_inventory_event from a parsed kex
/// summary. `host` and `identity` are supplied by the caller (the
/// pcap frame handler) the same way the TLS path builds them.
pub fn build_ssh_event(
    host: String,
    identity: String,
    summary: &SshKexSummary,
) -> CryptoInventoryEvent {
    let primitives = summary.primitives();
    let kex = summary.reported_kex().unwrap_or("<none>");
    let rationale = match summary.banner.as_deref() {
        Some(b) => format!(
            "SSH KEXINIT [{}]: kex={} ({} offered)",
            b,
            kex,
            summary.kex_algorithms.len()
        ),
        None => format!(
            "SSH KEXINIT: kex={} ({} offered)",
            kex,
            summary.kex_algorithms.len()
        ),
    };
    // Observed, not scored — the posture rollup assigns the real
    // score downstream from the primitives' pq_resistant flags.
    CryptoInventoryEvent {
        schema_version: SCHEMA_VERSION,
        schema_minor: SCHEMA_MINOR,
        source_module: crate::MODULE_NAME.into(),
        observed_at: chrono::Utc::now(),
        asset: Asset {
            kind: AssetKind::SshSession,
            identity,
            host: Some(host),
        },
        primitives,
        channel_protection: None,
        agility: None,
        posture: Posture {
            score: 50,
            rationale,
            recommended_replacement: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a KEXINIT binary packet carrying the given kex name-list
    /// plus eight more (empty) name-lists and the trailing
    /// first_kex_packet_follows + reserved fields, framed with a
    /// packet header. Returns the full on-wire bytes.
    fn kexinit_packet(kex: &[&str]) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.push(SSH_MSG_KEXINIT);
        payload.extend_from_slice(&[0u8; 16]); // cookie

        let write_list = |p: &mut Vec<u8>, items: &[&str]| {
            let joined = items.join(",");
            p.extend_from_slice(&(joined.len() as u32).to_be_bytes());
            p.extend_from_slice(joined.as_bytes());
        };
        // kex_algorithms (the one we read) + 9 more required lists.
        write_list(&mut payload, kex);
        for _ in 0..9 {
            write_list(&mut payload, &[]);
        }
        payload.push(0); // first_kex_packet_follows
        payload.extend_from_slice(&[0u8; 4]); // reserved

        // Frame: packet_length(4) padding_length(1) payload padding.
        // Pad to an 8-byte block (min 4 bytes of padding per RFC).
        let pad_len = {
            let unpadded = 1 + payload.len(); // padding_length byte + payload
            let block = 8;
            let mut pl = block - (unpadded % block);
            if pl < 4 {
                pl += block;
            }
            pl
        };
        let packet_len = 1 + payload.len() + pad_len;
        let mut out = Vec::new();
        out.extend_from_slice(&(packet_len as u32).to_be_bytes());
        out.push(pad_len as u8);
        out.extend_from_slice(&payload);
        out.extend(std::iter::repeat(0u8).take(pad_len));
        out
    }

    fn with_banner(banner: &str, pkt: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(banner.as_bytes());
        v.extend_from_slice(b"\r\n");
        v.extend_from_slice(pkt);
        v
    }

    #[test]
    fn hybrid_mlkem_is_pq() {
        let pkt = kexinit_packet(&[
            "mlkem768x25519-sha256",
            "curve25519-sha256",
            "ecdh-sha2-nistp256",
        ]);
        let buf = with_banner("SSH-2.0-OpenSSH_9.9", &pkt);
        let s = parse_ssh(&buf).unwrap();
        assert_eq!(s.banner.as_deref(), Some("SSH-2.0-OpenSSH_9.9"));
        assert!(s.offers_pq());
        assert_eq!(s.reported_kex(), Some("mlkem768x25519-sha256"));
        let p = s.primitives();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].algorithm, "mlkem768x25519");
        assert_eq!(p[0].pq_resistant, Some(true));
        assert_eq!(p[0].nist_classification, Some(NistLevel::L3));
    }

    #[test]
    fn sntrup_openssh_suffix_is_pq() {
        let pkt = kexinit_packet(&["sntrup761x25519-sha512@openssh.com", "curve25519-sha256"]);
        let s = parse_ssh(&pkt).unwrap();
        assert!(s.offers_pq());
        assert_eq!(s.primitives()[0].algorithm, "sntrup761x25519");
        assert_eq!(s.primitives()[0].nist_classification, Some(NistLevel::L5));
    }

    #[test]
    fn classical_only_is_not_pq() {
        let pkt = kexinit_packet(&["curve25519-sha256", "ecdh-sha2-nistp256"]);
        let buf = with_banner("SSH-2.0-OpenSSH_8.2p1 Ubuntu-4", &pkt);
        let s = parse_ssh(&buf).unwrap();
        assert!(!s.offers_pq());
        assert_eq!(s.reported_kex(), Some("curve25519-sha256"));
        assert_eq!(s.primitives()[0].pq_resistant, Some(false));
        assert!(s.primitives()[0].nist_classification.is_none());
    }

    #[test]
    fn banner_only_no_kexinit() {
        // Banner present but KEXINIT not in this slice.
        let buf = b"SSH-2.0-OpenSSH_9.6\r\n".to_vec();
        let s = parse_ssh(&buf).unwrap();
        assert_eq!(s.banner.as_deref(), Some("SSH-2.0-OpenSSH_9.6"));
        assert!(s.kex_algorithms.is_empty());
        assert!(s.reported_kex().is_none());
    }

    #[test]
    fn kexinit_without_banner() {
        // KEXINIT only (banner was in an earlier dropped segment).
        let pkt = kexinit_packet(&["mlkem768x25519-sha256"]);
        let s = parse_ssh(&pkt).unwrap();
        assert!(s.banner.is_none());
        assert!(s.offers_pq());
    }

    #[test]
    fn non_ssh_rejected() {
        let buf = b"GET / HTTP/1.1\r\nHost: x\r\n\r\n".to_vec();
        assert_eq!(parse_ssh(&buf), Err(SshParseError::NotSsh));
    }

    #[test]
    fn truncated_name_list_is_malformed() {
        // A KEXINIT whose kex name-list length overruns the payload.
        let mut payload = vec![SSH_MSG_KEXINIT];
        payload.extend_from_slice(&[0u8; 16]);
        payload.extend_from_slice(&(9999u32).to_be_bytes()); // absurd list len
        payload.extend_from_slice(b"curve25519");
        let pad_len = 4usize;
        let packet_len = 1 + payload.len() + pad_len;
        let mut buf = Vec::new();
        buf.extend_from_slice(&(packet_len as u32).to_be_bytes());
        buf.push(pad_len as u8);
        buf.extend_from_slice(&payload);
        buf.extend(std::iter::repeat(0u8).take(pad_len));
        assert_eq!(parse_ssh(&buf), Err(SshParseError::Malformed));
    }

    #[test]
    fn event_shape_is_ssh_session() {
        let pkt = kexinit_packet(&["mlkem768x25519-sha256"]);
        let s = parse_ssh(&pkt).unwrap();
        let ev = build_ssh_event("10.0.0.5:22".into(), "live-abc".into(), &s);
        assert_eq!(ev.asset.kind, AssetKind::SshSession);
        assert_eq!(ev.asset.host.as_deref(), Some("10.0.0.5:22"));
        assert_eq!(ev.primitives[0].algorithm, "mlkem768x25519");
    }
}
