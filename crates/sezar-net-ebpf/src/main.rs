//! Sezar — kernel-side eBPF program.
//!
//! TC classifier attached to an interface's ingress hook. For every
//! packet whose TCP payload starts with a TLS Plaintext handshake
//! record (`0x16 0x03 0x0X` + msg_type `0x01`/`0x02`), copy up to
//! [`MAX_HANDSHAKE_BYTES`] into a ring-buffer entry along with the
//! TCP 4-tuple. The userspace loader (`sezar-net live-iface`,
//! gated behind the `live-interface` feature) consumes the
//! ring buffer and emits one `crypto_inventory_event` per parsed
//! handshake.
//!
//! # Why a TC classifier (and not socket / kprobe / uprobe)
//!
//! - **Socket filter**: sees only packets that match a BPF program
//!   on a specific `AF_PACKET` socket. Less ergonomic for
//!   long-running passive observability.
//! - **kprobe `tcp_recvmsg`**: sees post-reassembly buffers, but the
//!   per-packet hook is a better fit for ClientHello / ServerHello
//!   which are small and typically fit one segment.
//! - **uprobe on `SSL_read`**: app-specific (OpenSSL only), needs
//!   per-process attachment, sees post-decryption bytes (more than
//!   we need for posture observation).
//!
//! TC classifier on ingress is the right level: see every packet,
//! cheap parse, no per-app instrumentation.
//!
//! # Build
//!
//! ```bash
//! rustup target add bpfel-unknown-none --toolchain nightly
//! cargo install bpf-linker
//! cargo +nightly build -Z build-std=core --release \
//!       --target bpfel-unknown-none
//! ```
//!
//! Resulting object: `target/bpfel-unknown-none/release/sezar-net-ebpf`.

#![no_std]
#![no_main]
#![allow(nonstandard_style, dead_code)]

use aya_ebpf::{
    bindings::TC_ACT_PIPE,
    macros::{classifier, map},
    maps::RingBuf,
    programs::TcContext,
};
use aya_log_ebpf::info;

/// Maximum handshake bytes we copy per packet. ClientHellos with
/// modest extension lists fit well below 1 KiB; the cap keeps a
/// single ring-buffer entry to a single 4 KiB page.
pub const MAX_HANDSHAKE_BYTES: usize = 1024;

/// Ring buffer feeding userspace.
#[map]
static EVENTS: RingBuf = RingBuf::with_byte_size(1 << 20, 0);

/// Per-packet event pushed to userspace.
#[repr(C)]
pub struct HandshakeEvent {
    /// Source IPv4 address (network order).
    pub src_ip: u32,
    /// Destination IPv4 address (network order).
    pub dst_ip: u32,
    /// Source port (host order).
    pub src_port: u16,
    /// Destination port (host order).
    pub dst_port: u16,
    /// How many bytes of `bytes` are valid.
    pub len: u16,
    /// Padding for 4-byte alignment.
    pub _pad: u16,
    /// TLS handshake bytes (from the inner msg_type onward).
    pub bytes: [u8; MAX_HANDSHAKE_BYTES],
}

/// TC ingress classifier.
#[classifier]
pub fn sezar_net_tc(ctx: TcContext) -> i32 {
    // Pipe = let the packet through regardless of our success or
    // failure. We're observing, not filtering.
    match try_observe(&ctx) {
        Ok(()) => TC_ACT_PIPE,
        Err(_) => TC_ACT_PIPE,
    }
}

#[inline(always)]
fn try_observe(ctx: &TcContext) -> Result<(), ()> {
    // Parse Ethernet header.
    const ETH_HDR_LEN: usize = 14;
    const ETH_P_IP: u16 = 0x0800;
    let eth_proto = u16::from_be_bytes(read_bytes::<2>(ctx, 12)?);
    if eth_proto != ETH_P_IP {
        return Ok(());
    }

    // Parse IPv4 header.
    let ip_first = read_byte(ctx, ETH_HDR_LEN)?;
    let ip_ihl = (ip_first & 0x0f) as usize * 4;
    if ip_ihl < 20 {
        return Ok(());
    }
    let ip_proto = read_byte(ctx, ETH_HDR_LEN + 9)?;
    const IPPROTO_TCP: u8 = 6;
    if ip_proto != IPPROTO_TCP {
        return Ok(());
    }
    let src_ip = u32::from_be_bytes(read_bytes::<4>(ctx, ETH_HDR_LEN + 12)?);
    let dst_ip = u32::from_be_bytes(read_bytes::<4>(ctx, ETH_HDR_LEN + 16)?);

    // Parse TCP header.
    let tcp_off = ETH_HDR_LEN + ip_ihl;
    let src_port = u16::from_be_bytes(read_bytes::<2>(ctx, tcp_off)?);
    let dst_port = u16::from_be_bytes(read_bytes::<2>(ctx, tcp_off + 2)?);
    let data_off_byte = read_byte(ctx, tcp_off + 12)?;
    let tcp_hdr_len = ((data_off_byte >> 4) as usize) * 4;
    if tcp_hdr_len < 20 {
        return Ok(());
    }

    // TCP payload.
    let payload_off = tcp_off + tcp_hdr_len;
    // TLS record header: 0x16 (handshake), 0x03, 0x0X.
    let rec_type = read_byte(ctx, payload_off)?;
    if rec_type != 0x16 {
        return Ok(());
    }
    let major = read_byte(ctx, payload_off + 1)?;
    if major != 0x03 {
        return Ok(());
    }
    // Inner handshake msg_type at payload[5].
    let msg_type = read_byte(ctx, payload_off + 5)?;
    if msg_type != 0x01 && msg_type != 0x02 {
        return Ok(());
    }

    // Submit to ring buffer. We copy from the inner handshake start
    // (payload + 5) up to MAX_HANDSHAKE_BYTES.
    let body_off = payload_off + 5;
    if let Some(mut entry) = EVENTS.reserve::<HandshakeEvent>(0) {
        let ev = unsafe { entry.as_mut_ptr().as_mut() };
        if let Some(ev) = ev {
            ev.src_ip = src_ip;
            ev.dst_ip = dst_ip;
            ev.src_port = src_port;
            ev.dst_port = dst_port;
            ev._pad = 0;
            ev.len = 0;
            for i in 0..MAX_HANDSHAKE_BYTES {
                match read_byte(ctx, body_off + i) {
                    Ok(b) => {
                        ev.bytes[i] = b;
                        ev.len = (i as u16) + 1;
                    }
                    Err(_) => break,
                }
            }
            info!(ctx, "submitted handshake event ({} bytes)", ev.len);
        }
        entry.submit(0);
    }

    Ok(())
}

#[inline(always)]
fn read_byte(ctx: &TcContext, off: usize) -> Result<u8, ()> {
    ctx.load::<u8>(off).map_err(|_| ())
}

#[inline(always)]
fn read_bytes<const N: usize>(ctx: &TcContext, off: usize) -> Result<[u8; N], ()> {
    let mut out = [0u8; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = ctx.load::<u8>(off + i).map_err(|_| ())?;
    }
    Ok(out)
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    // The aya verifier rejects unreachable_unchecked; loop is the
    // canonical panic implementation for no_std BPF programs.
    loop {}
}
