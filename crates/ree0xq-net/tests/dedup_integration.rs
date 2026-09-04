//! Integration test for the recent-session dedup cache.
//!
//! Builds a tiny pcap containing the same TLS 1.3 ClientHello
//! frame replayed three times (same 4-tuple) and asserts that with
//! a dedup cache attached only the first observation produces an
//! event; the second and third are accounted for in
//! `stats.handshakes_deduplicated`.
//!
//! Re-uses the synthetic ClientHello + pcap-frame builders from
//! `live_pcap.rs` rather than refactoring them into a shared
//! helper — they're test scaffolding, not part of the crate's
//! public API.

use std::io::Cursor;
use std::time::Duration;

use pcap_file::pcap::{PcapHeader, PcapPacket, PcapWriter};
use pcap_file::DataLink;
use ree0xq_net::dedup::DedupCache;
use ree0xq_net::live;

mod helpers {
    pub fn sample_client_hello() -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&[0x03, 0x03]);
        body.extend_from_slice(&[0u8; 32]);
        body.push(0);
        body.extend_from_slice(&(4u16).to_be_bytes());
        body.extend_from_slice(&[0x13, 0x02, 0x13, 0x01]);
        body.push(1);
        body.push(0);

        let mut ext = Vec::new();
        let mut sg = Vec::new();
        sg.extend_from_slice(&(4u16).to_be_bytes());
        sg.extend_from_slice(&[0x11, 0xec, 0x00, 0x1d]);
        ext.extend_from_slice(&0x000au16.to_be_bytes());
        ext.extend_from_slice(&(sg.len() as u16).to_be_bytes());
        ext.extend_from_slice(&sg);

        let mut sa = Vec::new();
        sa.extend_from_slice(&(4u16).to_be_bytes());
        sa.extend_from_slice(&[0x09, 0x05, 0x04, 0x03]);
        ext.extend_from_slice(&0x000du16.to_be_bytes());
        ext.extend_from_slice(&(sa.len() as u16).to_be_bytes());
        ext.extend_from_slice(&sa);

        let mut sv = Vec::new();
        sv.push(2u8);
        sv.extend_from_slice(&[0x03, 0x04]);
        ext.extend_from_slice(&0x002bu16.to_be_bytes());
        ext.extend_from_slice(&(sv.len() as u16).to_be_bytes());
        ext.extend_from_slice(&sv);

        body.extend_from_slice(&(ext.len() as u16).to_be_bytes());
        body.extend_from_slice(&ext);

        let blen = body.len() as u32;
        let mut hs = Vec::new();
        hs.push(0x01u8);
        hs.extend_from_slice(&[(blen >> 16) as u8, (blen >> 8) as u8, blen as u8]);
        hs.extend_from_slice(&body);
        hs
    }

    pub fn ipv4_checksum(hdr: &[u8]) -> u16 {
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

    /// Build a frame with arbitrary src/dst IPv4 + TCP ports so the
    /// dedup test can swap the 4-tuple between frames.
    pub fn build_frame(
        src_ip: [u8; 4],
        src_port: u16,
        dst_ip: [u8; 4],
        dst_port: u16,
        body: &[u8],
    ) -> Vec<u8> {
        let mut frame = Vec::new();
        frame.extend_from_slice(&[0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
        frame.extend_from_slice(&[0x52, 0x54, 0x00, 0x00, 0x00, 0x01]);
        frame.extend_from_slice(&0x0800u16.to_be_bytes());

        let tls_record_len = body.len() as u16;
        let mut tls_record = Vec::with_capacity(5 + body.len());
        tls_record.push(0x16);
        tls_record.push(0x03);
        tls_record.push(0x03);
        tls_record.extend_from_slice(&tls_record_len.to_be_bytes());
        tls_record.extend_from_slice(body);

        let tcp_payload_len = tls_record.len();
        let total_ipv4 = 20 + 20 + tcp_payload_len;

        let mut ip = Vec::with_capacity(20);
        ip.push(0x45);
        ip.push(0x00);
        ip.extend_from_slice(&(total_ipv4 as u16).to_be_bytes());
        ip.extend_from_slice(&0u16.to_be_bytes());
        ip.extend_from_slice(&0x4000u16.to_be_bytes());
        ip.push(64);
        ip.push(6);
        ip.extend_from_slice(&0u16.to_be_bytes());
        ip.extend_from_slice(&src_ip);
        ip.extend_from_slice(&dst_ip);
        let cksum = ipv4_checksum(&ip);
        ip[10..12].copy_from_slice(&cksum.to_be_bytes());
        frame.extend_from_slice(&ip);

        let mut tcp = Vec::with_capacity(20);
        tcp.extend_from_slice(&src_port.to_be_bytes());
        tcp.extend_from_slice(&dst_port.to_be_bytes());
        tcp.extend_from_slice(&0u32.to_be_bytes());
        tcp.extend_from_slice(&0u32.to_be_bytes());
        tcp.push(0x50);
        tcp.push(0x18);
        tcp.extend_from_slice(&65535u16.to_be_bytes());
        tcp.extend_from_slice(&0u16.to_be_bytes());
        tcp.extend_from_slice(&0u16.to_be_bytes());
        frame.extend_from_slice(&tcp);

        frame.extend_from_slice(&tls_record);
        frame
    }
}

fn write_pcap(frames: &[Vec<u8>]) -> Vec<u8> {
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
        for (i, f) in frames.iter().enumerate() {
            writer
                .write_packet(&PcapPacket::new(
                    Duration::from_secs(1_700_000_000 + i as u64),
                    f.len() as u32,
                    f,
                ))
                .unwrap();
        }
    }
    buf
}

#[test]
fn dedup_drops_retransmits_on_same_4tuple() {
    let body = helpers::sample_client_hello();
    // Three frames, identical 4-tuple — same session retransmitting.
    let frames = vec![
        helpers::build_frame([10, 0, 0, 1], 50001, [203, 0, 113, 7], 443, &body),
        helpers::build_frame([10, 0, 0, 1], 50001, [203, 0, 113, 7], 443, &body),
        helpers::build_frame([10, 0, 0, 1], 50001, [203, 0, 113, 7], 443, &body),
    ];
    let pcap_bytes = write_pcap(&frames);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("retransmit.pcap");
    std::fs::write(&p, &pcap_bytes).unwrap();

    let mut cache = DedupCache::default();
    let mut events = Vec::new();
    let stats = live::observe_pcap_with_dedup(&p, Some(&mut cache), |ev| events.push(ev)).unwrap();

    assert_eq!(stats.packets_seen, 3);
    assert_eq!(stats.handshake_packets, 3);
    assert_eq!(stats.handshakes_parsed, 3);
    assert_eq!(
        stats.events_emitted, 1,
        "retransmits should be deduplicated"
    );
    assert_eq!(stats.handshakes_deduplicated, 2);
    assert_eq!(events.len(), 1);
}

#[test]
fn distinct_4tuples_each_emit_through_dedup() {
    let body = helpers::sample_client_hello();
    // Three frames, distinct source ports — three independent
    // sessions to the same server. Dedup must not merge them.
    let frames = vec![
        helpers::build_frame([10, 0, 0, 1], 50001, [203, 0, 113, 7], 443, &body),
        helpers::build_frame([10, 0, 0, 1], 50002, [203, 0, 113, 7], 443, &body),
        helpers::build_frame([10, 0, 0, 2], 50001, [203, 0, 113, 7], 443, &body),
    ];
    let pcap_bytes = write_pcap(&frames);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("distinct.pcap");
    std::fs::write(&p, &pcap_bytes).unwrap();

    let mut cache = DedupCache::default();
    let mut events = Vec::new();
    let stats = live::observe_pcap_with_dedup(&p, Some(&mut cache), |ev| events.push(ev)).unwrap();

    assert_eq!(stats.events_emitted, 3);
    assert_eq!(stats.handshakes_deduplicated, 0);
    assert_eq!(events.len(), 3);
}

#[test]
fn no_dedup_cache_preserves_legacy_emit_everything_behaviour() {
    let body = helpers::sample_client_hello();
    let frames = vec![
        helpers::build_frame([10, 0, 0, 1], 50001, [203, 0, 113, 7], 443, &body),
        helpers::build_frame([10, 0, 0, 1], 50001, [203, 0, 113, 7], 443, &body),
    ];
    let pcap_bytes = write_pcap(&frames);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("nodedup.pcap");
    std::fs::write(&p, &pcap_bytes).unwrap();

    let mut events = Vec::new();
    let stats = live::observe_pcap(&p, |ev| events.push(ev)).unwrap();

    // Without a cache attached, retransmits all surface as events.
    assert_eq!(stats.events_emitted, 2);
    assert_eq!(stats.handshakes_deduplicated, 0);
    assert_eq!(events.len(), 2);
}
