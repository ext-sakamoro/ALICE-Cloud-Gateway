# Contributing to ALICE-Cloud-Gateway

## Build

```bash
cargo build
```

## Test

```bash
cargo test
```

## Lint

```bash
cargo clippy -- -W clippy::all
cargo fmt -- --check
cargo doc --no-deps 2>&1 | grep warning
```

## Design Constraints

- **Edge-to-cloud pipeline**: receives ASP packets from edge devices, decrypts, and routes to subsystems.
- **ALICE ecosystem integration**: connects ALICE-DB (SDF storage), ALICE-Cache, ALICE-Sync (cloud), ALICE-CDN.
- **Device key derivation**: session keys derived from master secret via ALICE-Crypto.
- **Tokio async runtime**: all I/O is async; `parking_lot` for synchronous shared state.
