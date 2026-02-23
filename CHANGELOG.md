# Changelog

All notable changes to ALICE-Cloud-Gateway will be documented in this file.

## [0.1.0] - 2026-02-23

### Added
- `ingest` — UDP/QUIC packet ingestion, ASP frame decryption, route to DB/Cache/Sync/CDN
- `device_keys` — Device key derivation and session management via ALICE-Crypto
- `telemetry` — Gateway metrics bridge to ALICE-Analytics
- `queue_bridge` — Async message queue integration
- `container_bridge` — ALICE-Container resource scheduling bridge
- `GatewayConfig` with defaults (listen 0.0.0.0:4433, 64KB max packet, 100K cache entries)
- 82 unit tests
