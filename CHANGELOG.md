# Changelog

All notable changes to ALICE-Cloud-Gateway will be documented in this file.

## [Unreleased]

### Security
- CI: `security-audit.yml` を追加 `.github/alice-siblings` に固定した実物の sibling (Analytics / Cache / CDN / Crypto / DB / Streaming-Protocol / Sync / Zip) と毎回新しい解決で `cargo audit` と `cargo deny --all-features check all` を走らせ、解決と deny の graph がこの crate と path の sibling を全て含むこと (0 件は失敗) を検査する 週次は土曜 03:17 UTC
- 解消に依存の更新が要る勧告は `scripts/advisory_holds.toml` の期限付きの保留 (2026-11-14 まで) とし、`scripts/advisory_hold_guard.py` が版と依存元の一致と期限を検査する: rkyv 0.7.46 (RUSTSEC-2026-0235、alice-db 経由) / bincode 1.3.3 (unmaintained、alice-db と alice-sync 経由) それ以外の勧告は保留せず失敗する
- `deny.toml`: この crate と AGPL の sibling (cdn / cache / crypto / db / sync) の AGPL 側を crate 単位の exception で許可

### Changed
- Licence: `AGPL-3.0-only OR LicenseRef-Commercial` → `LicenseRef-Proprietary`
- License の SPDX 識別子を非推奨の `AGPL-3.0` から `AGPL-3.0-only` に (`Cargo.toml` の `AGPL-3.0-only OR LicenseRef-Commercial` と README の表記) 条文と利用条件は不変 `deny.toml` の alice-cloud-gateway / alice-cdn の exception も `AGPL-3.0-only` にし、`.github/alice-siblings` の ALICE-CDN を同じ変更の入った commit (6e3a9e5) に進める
- CI: `ci.yml` は `ci/**` の branch と手動実行でも走る main の run は打ち切らず、`ci/**` の古い run は新しい run が来たら打ち切る
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
