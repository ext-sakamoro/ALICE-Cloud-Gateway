# Changelog

All notable changes to ALICE-Cloud-Gateway will be documented in this file.

## [Unreleased]

### Security
- CI: `security-audit.yml` を追加 `.github/alice-siblings` に固定した実物の sibling (Analytics / Cache / CDN / Crypto / DB / Streaming-Protocol / Sync / Zip) と毎回新しい解決で `cargo audit` と `cargo deny --all-features check all` を走らせ、解決と deny の graph がこの crate と path の sibling を全て含むこと (0 件は失敗) を検査する 週次は土曜 03:17 UTC
- 解消に依存の更新が要る勧告は `scripts/advisory_holds.toml` の期限付きの保留 (2026-11-14 まで) とし、`scripts/advisory_hold_guard.py` が版と依存元の一致と期限を検査する: bincode 1.3.3 (unmaintained、alice-db と alice-sync 経由) それ以外の勧告は保留せず失敗する

### Changed
- CI: `.github/actions/alice-siblings/action.yml` を共有の版 (ALICE-Analytics の main と同じ bytes) に揃える `.github/alice-siblings-private` に載せた非公開の隣を読み取り専用の鍵で clone できるようになった (この repository の隣はすべて公開なので、clone の仕方は変わらない)
- Licence: `AGPL-3.0` → `LicenseRef-Proprietary`
- 依存: sibling の ALICE-Cache / CDN / Crypto / DB / Sync を現在の main に (deny の例外は `LicenseRef-Proprietary`)、`[patch.crates-io]` で alice-det-math を ALICE-DetMath `6384856` (0.5.2) の 1 つにする
- CI: `ci.yml` は `ci/**` の branch と手動実行でも走る main の run は打ち切らず、`ci/**` の古い run は新しい run が来たら打ち切る

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
