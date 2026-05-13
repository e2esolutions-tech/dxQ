#!/usr/bin/env python3
"""Study 1 — small-sample TLS probe.

For each host in hosts.txt:
  - Open a TCP connection to host:443
  - Perform a TLS handshake using the system default openssl
  - Record the negotiated version, cipher, peer certificate sig alg
  - Build a sezar_core::CryptoInventoryEvent-shaped dict

Ethics:
  - ≤1 TCP connection per host
  - 5-second connect + 5-second handshake timeout
  - 1Hz rate cap (single-threaded sequential loop)
  - Hosts are well-known public sites where a single TLS handshake
    is operationally a non-event

Outputs:
  captures/scan.json       — per-host probe results
  captures/events.ndjson   — one CryptoInventoryEvent per host (NDJSON)
"""

from __future__ import annotations

import json
import socket
import ssl
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives.asymmetric import (
    rsa, ec, ed25519, ed448, dsa,
)

ROOT = Path(__file__).resolve().parent
HOSTS = (ROOT / "hosts.txt").read_text().strip().splitlines()
CAPTURE_DIR = ROOT / "captures"
CAPTURE_DIR.mkdir(exist_ok=True)


# ---------------- ciphersuite → primitives table (subset) ----------------
# Mirrors crates/sezar-net/src/algos.rs (TLS 1.3 + common TLS 1.2)
TLS13_NAME_FIX = {
    # Python's ssl reports IANA-style names with `TLS_` prefix.
    # openssl-style "TLS_AES_256_GCM_SHA384" is what we expect already.
}


def tls13_primitives(cipher_name: str):
    out = []
    encrypt_map = {
        "TLS_AES_128_GCM_SHA256": ("AES-128-GCM", False),
        "TLS_AES_256_GCM_SHA384": ("AES-256-GCM", True),
        "TLS_CHACHA20_POLY1305_SHA256": ("ChaCha20-Poly1305", True),
        "TLS_AES_128_CCM_SHA256": ("AES-128-CCM", False),
    }
    hash_map = {
        "SHA256": ("SHA-256", True),
        "SHA384": ("SHA-384", True),
    }
    if cipher_name in encrypt_map:
        algo, pq = encrypt_map[cipher_name]
        out.append({"role": "encrypt", "algorithm": algo, "pq_resistant": pq})
    for tail, (h, pq) in hash_map.items():
        if cipher_name.endswith(tail):
            out.append({"role": "hash", "algorithm": h, "pq_resistant": pq})
            break
    return out


def tls12_primitives(cipher_name: str):
    out = []
    if "_ECDHE_" in cipher_name:
        out.append({"role": "kex", "algorithm": "ECDHE", "pq_resistant": False})
    elif "_DHE_" in cipher_name:
        out.append({"role": "kex", "algorithm": "DHE", "pq_resistant": False})
    elif "_ECDH_" in cipher_name:
        out.append({"role": "kex", "algorithm": "ECDH", "pq_resistant": False})
    elif "_RSA_" in cipher_name and "_DHE_" not in cipher_name and "_ECDHE_" not in cipher_name:
        out.append({"role": "kex", "algorithm": "RSA-KEX", "pq_resistant": False})

    if "_ECDSA_" in cipher_name:
        out.append({"role": "sig", "algorithm": "ECDSA", "pq_resistant": False})
    elif "_RSA_" in cipher_name:
        out.append({"role": "sig", "algorithm": "RSA", "pq_resistant": False})

    if "_AES_256_GCM" in cipher_name:
        out.append({"role": "encrypt", "algorithm": "AES-256-GCM", "pq_resistant": True})
    elif "_AES_128_GCM" in cipher_name:
        out.append({"role": "encrypt", "algorithm": "AES-128-GCM", "pq_resistant": False})
    elif "_AES_256_CBC" in cipher_name:
        out.append({"role": "encrypt", "algorithm": "AES-256-CBC", "pq_resistant": False})
    elif "_AES_128_CBC" in cipher_name:
        out.append({"role": "encrypt", "algorithm": "AES-128-CBC", "pq_resistant": False})
    elif "_CHACHA20_POLY1305" in cipher_name:
        out.append({"role": "encrypt", "algorithm": "ChaCha20-Poly1305", "pq_resistant": True})
    elif "_3DES_" in cipher_name:
        out.append({"role": "encrypt", "algorithm": "3DES", "pq_resistant": False})

    if cipher_name.endswith("_SHA384"):
        out.append({"role": "hash", "algorithm": "SHA-384", "pq_resistant": True})
    elif cipher_name.endswith("_SHA256"):
        out.append({"role": "hash", "algorithm": "SHA-256", "pq_resistant": True})
    elif cipher_name.endswith("_SHA"):
        out.append({"role": "hash", "algorithm": "SHA-1", "pq_resistant": False})
    return out


def cert_sig_primitive(cert: x509.Certificate):
    """Pick a single primitive describing the leaf cert's signature."""
    pub = cert.public_key()
    sig_hash = cert.signature_hash_algorithm.name if cert.signature_hash_algorithm else "unknown"
    sig_hash = sig_hash.upper()
    if isinstance(pub, rsa.RSAPublicKey):
        bits = pub.key_size
        if bits <= 1024:
            algo = f"RSA-PKCS1-{sig_hash} (RSA-{bits})"
        else:
            algo = f"RSA-PKCS1-{sig_hash}"
    elif isinstance(pub, ec.EllipticCurvePublicKey):
        curve = pub.curve.name
        algo_map = {"secp256r1": "ECDSA-P256", "secp384r1": "ECDSA-P384", "secp521r1": "ECDSA-P521"}
        algo = algo_map.get(curve, f"ECDSA-{curve}")
    elif isinstance(pub, ed25519.Ed25519PublicKey):
        algo = "Ed25519"
    elif isinstance(pub, ed448.Ed448PublicKey):
        algo = "Ed448"
    elif isinstance(pub, dsa.DSAPublicKey):
        algo = "DSA"
    else:
        algo = "unknown"
    return {"role": "sig", "algorithm": algo, "pq_resistant": False,
            "parameters": {"signature_hash": sig_hash}}


def probe(host: str, timeout: float = 5.0):
    ctx = ssl.create_default_context()
    # We accept whatever the server advertises; we do not enforce
    # certificate validity (some hosts may have stale chains we don't
    # care about for crypto-posture observability).
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    ctx.minimum_version = ssl.TLSVersion.TLSv1_2
    info = {
        "host": host,
        "ok": False,
        "version": None,
        "cipher": None,
        "cert_sig_algo": None,
        "primitives": [],
        "error": None,
    }
    try:
        with socket.create_connection((host, 443), timeout=timeout) as sock:
            with ctx.wrap_socket(sock, server_hostname=host) as ssock:
                info["version"] = ssock.version()
                cipher = ssock.cipher()
                if cipher:
                    info["cipher"] = cipher[0]
                der = ssock.getpeercert(binary_form=True)
                if der:
                    cert = x509.load_der_x509_certificate(der)
                    p = cert_sig_primitive(cert)
                    info["cert_sig_algo"] = p["algorithm"]
                    info["primitives"].append(p)
                info["ok"] = True
        # Build TLS handshake primitives from the negotiated ciphersuite.
        if info["cipher"]:
            if info["cipher"].startswith("TLS_AES_") or info["cipher"].startswith(
                "TLS_CHACHA20_"
            ):
                info["primitives"].extend(tls13_primitives(info["cipher"]))
            else:
                info["primitives"].extend(tls12_primitives(info["cipher"]))
    except Exception as e:
        info["error"] = f"{type(e).__name__}: {e}"
    return info


def build_event(info):
    return {
        "schema_version": 1,
        "schema_minor": 1,
        "source_module": "sezar-net/study1-probe",
        "observed_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "asset": {
            "kind": "tls_session",
            "identity": f"study1-{info['host']}",
            "host": info["host"],
        },
        "primitives": info["primitives"],
        "posture": {
            "score": 0,
            "rationale": (
                f"observed: cipher={info['cipher']} version={info['version']}"
                if info["ok"]
                else f"probe failed: {info['error']}"
            ),
            "recommended_replacement": None,
        },
    }


def main():
    out_scan = []
    out_events = []
    rate_delay = 1.0  # second per host — well under any sane rate limit
    for i, host in enumerate(HOSTS, 1):
        print(f"[{i:2}/{len(HOSTS)}] {host} ... ", end="", flush=True)
        info = probe(host)
        if info["ok"]:
            print(f"{info['version']} {info['cipher']} cert={info['cert_sig_algo']}")
        else:
            print(f"FAIL ({info['error']})")
        out_scan.append(info)
        out_events.append(build_event(info))
        time.sleep(rate_delay)

    (CAPTURE_DIR / "scan.json").write_text(json.dumps(out_scan, indent=2))
    with (CAPTURE_DIR / "events.ndjson").open("w") as f:
        for ev in out_events:
            f.write(json.dumps(ev) + "\n")
    print(f"\nwrote {len(out_scan)} probes to {CAPTURE_DIR}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
