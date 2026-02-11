# ALICE-Cloud-Gateway

Cloud gateway for the ALICE edge-to-cloud SDF streaming pipeline.

Receives encrypted ASP (ALICE Streaming Protocol) packets from edge devices (e.g. Raspberry Pi 5 + Dolphin D5 Lite), decrypts them, and routes data through the ALICE ecosystem subsystems.

## Architecture

```
Edge Device (ASP/QUIC)
       │
       ▼
┌─── ALICE-Cloud-Gateway ───────────────────────┐
│                                                │
│  UDP Listener (tokio async)                    │
│       │                                        │
│       ▼                                        │
│  IngestPipeline::process_packet()              │
│       │                                        │
│       ├─→ ALICE-Crypto   decrypt (BLAKE3 KDF)  │
│       ├─→ ALICE-DB       SDF spatial storage   │
│       ├─→ ALICE-Cache    hot frame cache       │
│       ├─→ ALICE-Sync     multi-device sync     │
│       ├─→ ALICE-CDN      edge routing          │
│       └─→ ALICE-Analytics telemetry            │
└────────────────────────────────────────────────┘
```

## Modules

| File | Description |
|---|---|
| `src/main.rs` | QUIC/UDP server, async packet receive loop |
| `src/ingest.rs` | Packet ingest pipeline: decrypt → parse → store → cache → sync → telemetry |
| `src/device_keys.rs` | Per-device encryption key derivation (BLAKE3 KDF) with caching |
| `src/telemetry.rs` | Streaming metrics via DDSketch, HyperLogLog, CountMinSketch |

## Dependencies

| Crate | Role |
|---|---|
| `alice-db` | SDF spatial storage (Morton code Z-order) |
| `alice-cache` | Hot frame LRU cache |
| `alice-sync` | Multi-device cloud sync hub |
| `alice-cdn` | SDF-aware CDN routing (Maglev + Vivaldi) |
| `alice-crypto` | Packet encryption/decryption (XChaCha20-Poly1305) |
| `alice-analytics` | Probabilistic telemetry (DDSketch, HLL, CMS) |
| `libasp` | ALICE Streaming Protocol packet format |

## Build

```bash
cargo build --release
```

## Run

```bash
RUST_LOG=info cargo run --release
```

Default listen address: `0.0.0.0:4433` (UDP)

## License

AGPL-3.0 — See [LICENSE](LICENSE) for details.

## Author

Moroya Sakamoto
