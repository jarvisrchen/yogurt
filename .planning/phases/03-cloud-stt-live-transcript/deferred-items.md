# Phase 03 — Deferred Items

Tracked out-of-scope discoveries from plan execution. Each item is logged here per the
GSD SCOPE BOUNDARY rule (don't auto-fix issues unrelated to the current task).

## From Plan 03-01

### D-INT-01: yogurt-audio integration test `tests/synthetic.rs` fails to compile without `--features synthetic`

**Found:** During final `cargo test --workspace` verification in Plan 03-01.

**Pre-existing:** Yes — reproduced on HEAD before any 03-01 changes were staged.

**Symptom:**

```
error[E0432]: unresolved import `yogurt_audio::synthetic`
  --> crates/yogurt-audio/tests/synthetic.rs:9:5
```

**Root cause:** `crates/yogurt-audio/src/lib.rs:29-30` gates the `synthetic` module on
`#[cfg(any(test, feature = "synthetic"))]`. The `test` cfg is set when compiling the
library's own unit tests (where the gate works) but is NOT set when compiling
`tests/synthetic.rs` as a separate integration-test crate — that crate sees only the
public-API surface of yogurt-audio compiled without the `synthetic` feature.

**Fix (when picked up):** Either

1. Add `required-features = ["synthetic"]` to a `[[test]]` entry in
   `crates/yogurt-audio/Cargo.toml` for the synthetic integration test, OR
2. Always export the module (drop the `cfg`-gate) since it's already test-only via
   feature flag.

Option 1 is the minimal, correct fix; option 2 is simpler but enlarges the default
compile graph.

**Scope:** yogurt-audio is locked from Phase 2 (per Plan 03-01 execution context). Not
touched in this plan.

**Workaround for Plan 03-01 verification:** `cargo test -p yogurt-stt` is the relevant
scope and passes 6/6 (1 lib + 4 deepgram unit + 1 integration). Workspace-wide
`cargo test --workspace` is blocked by this unrelated issue.
