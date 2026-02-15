# matrix-bridge-discord

This repository has been migrated from a Node.js/TypeScript baseline to a Rust-based implementation. The main code is now located in the `src/` directory at the repository root.

Maintainer: `Palpo Team`  
Contact: `chris@acroidea.com`

## Current Status (2026-02-15)

- Legacy Node.js/TypeScript code and build pipeline have been removed.
- Rust code is located in `src/` at the repository root and serves as the sole implementation.
- The root directory contains the main crate; Cargo commands can be executed directly at the repository root.
- Build pipeline restored: `cargo check -p matrix-bridge-discord` and `cargo test -p matrix-bridge-discord --no-run` pass.
- Web provisioning API no longer returns `501`; database read/write operations are integrated (create/query/delete/list bridges).

## Running and Verification

```bash
cargo check -p matrix-bridge-discord
cargo test -p matrix-bridge-discord --no-run
cargo run -p matrix-bridge-discord
```

## Notes

The repository and build system migration is complete.
Further development is needed for feature completeness (e.g., real Matrix/Discord SDK integration and full feature alignment). See `MIGRATION_STATUS.md` for details.
