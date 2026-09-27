# Changelog

All notable changes to ALICE-Cloud-Gateway will be documented in this file.

## [Unreleased]

### Changed
- **License: `AGPL-3.0` → `AGPL-3.0 OR LicenseRef-Commercial` (dual-licensed、2026-09-27)** AGPL 側の条件は変更なし (既存 AGPL 利用者への影響ゼロ)、商用という選択肢が追加されただけ SPDX が AGPL 単独だと cargo-deny / FOSSA / SBOM に「商用オプションなし」と見えるため宣言を dual に 変更点: SPDX / `LICENSE` → `LICENSE-AGPL` / `LICENSE-COMMERCIAL.md` (商用トリガー 6 条件 = クローズド製品・商用 SaaS・エッジ / ファームウェア配布・plugin 再配布・プラットフォーム NDA・保証、社内利用は AGPL 側で無償と明記) / README の選択肢表 商用窓口は法人 `contact@extoria.co.jp`

### Fixed
- `telemetry` — `CountMinSketch2048x7` / `DDSketch2048` / `FnvHasher` / `HyperLogLog16` を `alice_analytics::sketch::` から import (alice-analytics 0.1.0 で crate root の re-export が無くなり、real sibling では `cargo check` が止まっていた CI は stub crate で silent green)

## [0.1.0] - 2026-02-23

### Added
- `ingest` — UDP/QUIC packet ingestion, ASP frame decryption, route to DB/Cache/Sync/CDN
- `device_keys` — Device key derivation and session management via ALICE-Crypto
- `telemetry` — Gateway metrics bridge to ALICE-Analytics
- `queue_bridge` — Async message queue integration
- `container_bridge` — ALICE-Container resource scheduling bridge
- `GatewayConfig` with defaults (listen 0.0.0.0:4433, 64KB max packet, 100K cache entries)
- 82 unit tests
