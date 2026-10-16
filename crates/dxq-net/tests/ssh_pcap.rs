//! Integration test for `dxq_net::live::observe_pcap_ssh` (SEZ-30).
//!
//! Synthesises a pcap holding one Ethernet+IPv4+TCP frame from port
//! 22 whose payload is an SSH identification banner followed by a
//! server `SSH_MSG_KEXINIT` advertising `mlkem768x25519-sha256`,
//! then asserts the emitted event is an `ssh_session` carrying the
//! PQ-hybrid kex primitive. Also refreshes a committed fixture pcap
//! so docs/scripts can drive the CLI against known-good SSH bytes.

use std::io::Cursor;
use std::time::Duration;

use pcap_file::pcap::{PcapHeader, PcapPacket, PcapWriter};
use pcap_file::DataLink;
use dxq_net::live;

/// Build a KEXINIT binary packet carrying `kex` as the
/// kex_algorithms name-list (and 9 empty required lists), framed per
/// RFC 4253 §6/§7.1.
fn kexinit_packet(kex: &[&str]) -> Vec<u8> {
    const SSH_MSG_KEXINIT: u8 = 20;
    let mut payload = Vec::new();
    payload.push(SSH_MSG_KEXINIT);
    payload.extend_from_slice(&[0u8; 16]); // cookie
    let write_list = |p: &mut Vec<u8>, items: &[&str]| {
        let joined = items.join(",");
        p.extend_from_slice(&(joined.len() as u32).to_be_bytes());
        p.extend_from_slice(joined.as_bytes());
    };
    write_list(&mut payload, kex);
    for _ in 0..9 {
        write_list(&mut payload, &[]);
    }
    payload.push(0); // first_kex_packet_follows
    payload.extend_from_slice(&[0u8; 4]); // reserved

    let pad_len = {
        let unpadded = 1 + payload.len();
        let block = 8usize;
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

/// SSH payload = identification banner + KEXINIT, as the server
/// would send from port 22.
fn ssh_server_payload(kex: &[&str]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"SSH-2.0-OpenSSH_9.9\r\n");
    v.extend_from_slice(&kexinit_packet(kex));
    v
}

fn ipv4_checksum(hdr: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < hdr.len() {
        sum = sum.wrapping_add(u32::from(u16::from_be_bytes([hdr[i], hdr[i + 1]])));
        i += 2;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// Wrap an opaque TCP payload in Ethernet(14)+IPv4(20)+TCP(20),
/// source port 22 (server side), dst an ephemeral client port.
fn build_frame_from_port22(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(&[0x52, 0x54, 0x00, 0x12, 0x34, 0x56]); // dst mac
    frame.extend_from_slice(&[0x52, 0x54, 0x00, 0x00, 0x00, 0x01]); // src mac
    frame.extend_from_slice(&0x0800u16.to_be_bytes()); // IPv4

    let total_ipv4 = 20 + 20 + payload.len();
    let mut ip = Vec::with_capacity(20);
    ip.push(0x45);
    ip.push(0x00);
    ip.extend_from_slice(&(total_ipv4 as u16).to_be_bytes());
    ip.extend_from_slice(&0u16.to_be_bytes());
    ip.extend_from_slice(&0x4000u16.to_be_bytes());
    ip.push(64);
    ip.push(6); // TCP
    ip.extend_from_slice(&0u16.to_be_bytes());
    ip.extend_from_slice(&[203, 0, 113, 22]); // src = SSH server
    ip.extend_from_slice(&[10, 0, 0, 9]); // dst = client
    let cksum = ipv4_checksum(&ip);
    ip[10..12].copy_from_slice(&cksum.to_be_bytes());
    frame.extend_from_slice(&ip);

    let mut tcp = Vec::with_capacity(20);
    tcp.extend_from_slice(&22u16.to_be_bytes()); // src port 22
    tcp.extend_from_slice(&50002u16.to_be_bytes()); // dst ephemeral
    tcp.extend_from_slice(&0u32.to_be_bytes());
    tcp.extend_from_slice(&0u32.to_be_bytes());
    tcp.push(0x50);
    tcp.push(0x18);
    tcp.extend_from_slice(&65535u16.to_be_bytes());
    tcp.extend_from_slice(&0u16.to_be_bytes());
    tcp.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&tcp);

    frame.extend_from_slice(payload);
    frame
}

#[test]
fn observe_pcap_ssh_emits_pq_hybrid_event() {
    let mut buf = Vec::<u8>::new();
    {
        let header = PcapHeader {
            version_major: 2,
            version_minor: 4,
            ts_correction: 0,
            ts_accuracy: 0,
            snaplen: 65535,
            datalink: DataLink::ETHERNET,
            ts_resolution: pcap_file::TsResolution::MicroSecond,
            endianness: pcap_file::Endianness::Little,
        };
        let mut writer = PcapWriter::with_header(Cursor::new(&mut buf), header).unwrap();
        let payload = ssh_server_payload(&[
            "mlkem768x25519-sha256",
            "curve25519-sha256",
            "ecdh-sha2-nistp256",
        ]);
        let frame = build_frame_from_port22(&payload);
        writer
            .write_packet(&PcapPacket::new(
                Duration::from_secs(1_700_000_000),
                frame.len() as u32,
                &frame,
            ))
            .unwrap();
    }

    let dir = tempfile::tempdir().unwrap();
    let pcap_path = dir.path().join("ssh.pcap");
    std::fs::write(&pcap_path, &buf).unwrap();
    // Refresh committed fixture for CLI/doc use.
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/synth-ssh-kexinit.pcap");
    if let Some(parent) = fixture.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&fixture, &buf);

    let mut emitted = Vec::new();
    let stats = live::observe_pcap_ssh(&pcap_path, None, |ev| emitted.push(ev)).unwrap();

    assert_eq!(stats.packets_seen, 1);
    assert_eq!(stats.events_emitted, 1);
    assert_eq!(emitted.len(), 1);

    let ev = &emitted[0];
    assert_eq!(ev.source_module, "dxq-net");
    assert_eq!(ev.asset.kind, dxq_core::AssetKind::SshSession);
    // Server side is port 22 → host tagged as the SSH server.
    assert!(
        ev.asset.host.as_deref().unwrap().contains("203.0.113.22"),
        "host was {:?}",
        ev.asset.host
    );
    assert!(ev.asset.identity.starts_with("live-"));

    let kex = &ev.primitives[0];
    assert_eq!(kex.role, dxq_core::PrimitiveRole::Kex);
    assert_eq!(kex.algorithm, "mlkem768x25519");
    assert_eq!(kex.pq_resistant, Some(true));
}

#[test]
fn observe_pcap_ssh_ignores_non_ssh_and_classical_is_not_pq() {
    // Two frames: an HTTP payload (ignored) and a classical-only SSH
    // KEXINIT (emitted, not PQ).
    let mut buf = Vec::<u8>::new();
    {
        let header = PcapHeader {
            version_major: 2,
            version_minor: 4,
            ts_correction: 0,
            ts_accuracy: 0,
            snaplen: 65535,
            datalink: DataLink::ETHERNET,
            ts_resolution: pcap_file::TsResolution::MicroSecond,
            endianness: pcap_file::Endianness::Little,
        };
        let mut writer = PcapWriter::with_header(Cursor::new(&mut buf), header).unwrap();
        let http = build_frame_from_port22(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n");
        writer
            .write_packet(&PcapPacket::new(Duration::from_secs(1), http.len() as u32, &http))
            .unwrap();
        let ssh = build_frame_from_port22(&ssh_server_payload(&[
            "curve25519-sha256",
            "ecdh-sha2-nistp256",
        ]));
        writer
            .write_packet(&PcapPacket::new(Duration::from_secs(2), ssh.len() as u32, &ssh))
            .unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let pcap_path = dir.path().join("mixed.pcap");
    std::fs::write(&pcap_path, &buf).unwrap();

    let mut emitted = Vec::new();
    let stats = live::observe_pcap_ssh(&pcap_path, None, |ev| emitted.push(ev)).unwrap();
    assert_eq!(stats.packets_seen, 2);
    assert_eq!(emitted.len(), 1, "only the SSH frame should emit");
    let kex = &emitted[0].primitives[0];
    assert_eq!(kex.algorithm, "curve25519-sha256");
    assert_eq!(kex.pq_resistant, Some(false));
}
