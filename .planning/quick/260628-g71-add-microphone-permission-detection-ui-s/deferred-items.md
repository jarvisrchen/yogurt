# Deferred Items — 260628-g71

## Pre-existing `cargo fmt` violation in crates/yogurt-server/src/bootstrap.rs:88

The repo's current HEAD (`80b4473`) has a `cargo fmt` violation in
`crates/yogurt-server/src/bootstrap.rs` around line 88 — `existing.iter().find(…)`
needs to be inlined. Not caused by this plan; introduced by `e3d9169`
(`fix(server): backfill Keychain when env-seed finds row but missing key`).

Deferred — fixing it is out of scope for the mic-permission task per the
GSD scope-boundary rule. Suggest a one-line cleanup commit.
