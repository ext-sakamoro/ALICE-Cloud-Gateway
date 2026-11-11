# Changelog

All notable changes to ALICE-Cloud-Gateway will be documented in this file.

## [Unreleased]

### Security
- CI (security-audit): commit した `Cargo.lock` を検査する (以前は隣の repo を `.github/alice-siblings` の commit で取り寄せて毎回解決し直していた、取り寄せの action と固定の file は削除) `deny.toml` の `allow-git` に git で引く ALICE の repo を足した bincode が依存から消えた (alice-sync 0.8 / alice-db 0.6) ので RUSTSEC-2025-0141 の保留と ignore を除いた 共有の `advisory_hold_guard` と `sibling_coverage_guard` (試験も) を ALICE-Law cb12150 の計画の版にした: 保留が 0 件の時は ignore の一覧が空であることを確かめて通り、git で引く依存も sibling として数える (この crate では 7 件)
- CI: 共有の `advisory_hold_guard` (試験も) と `.github/actions/alice-siblings` を新しい版にした `deny.toml` は `unused-ignored-advisory = "deny"`
- CI: `security-audit.yml` を追加 `.github/alice-siblings` に固定した実物の sibling (Analytics / Cache / CDN / Crypto / DB / Streaming-Protocol / Sync / Zip) と毎回新しい解決で `cargo audit` と `cargo deny --all-features check all` を走らせ、解決と deny の graph がこの crate と path の sibling を全て含むこと (0 件は失敗) を検査する 週次は土曜 03:17 UTC
- 解消に依存の更新が要る勧告は `scripts/advisory_holds.toml` の期限付きの保留 (2026-11-14 まで) とし、`scripts/advisory_hold_guard.py` が版と依存元の一致と期限を検査する: bincode 1.3.3 (unmaintained、alice-db と alice-sync 経由) それ以外の勧告は保留せず失敗する

### Changed
- Cargo.lock: alice-db を main に取り直し、alice-db が前に rev で引いていた alice-serial も ALICE-Serial の main branch から (同じ commit 16b34eb) になった
- 依存: alice-db / alice-cache / alice-sync / alice-cdn / alice-crypto / alice-analytics / libasp を path から ALICE の各 repo の main (git、branch、版を指定) に変え、`[patch.crates-io]` の alice-det-math を除いた (どの依存も同じ det-math 0.6.1 を引く) `Cargo.lock` を追跡する CI の試験と clippy は中身の無い stub の crate に対する build (失敗を無視する設定つき) をやめ、実物の依存で `--locked` で走らせる (試験は ubuntu と macOS、通った試験が 148 件未満なら失敗、lock の alice-* がすべて git から来て alice-det-math が 1 つ以下であることも確かめる) `scripts/preflight.sh` は生成し直した
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
