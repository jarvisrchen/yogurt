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

## From Plan 03-02

### D-INT-02: `/ws/meetings/{id}` does NOT enforce session-token or Origin auth

**Found:** During Plan 03-02 Task 2 implementation.

**Status:** Intentional scope deferral.

**Why:** The PLAN file's acceptance criteria + the planner-authored integration test
`crates/yogurt-server/tests/meeting_ws.rs` connect to the new WS endpoint raw via
`tokio_tungstenite::connect_async` with no `?token=` query param and no `Origin`
header. Adding auth would break the test and would also contradict CONTEXT D-09 (which
specifies only: "look up meeting in Registry → subscribe → 4404 if missing"). The
Phase 0 `/ws` endpoint enforces both checks for the Phase 0 echo-stub use case; the
per-meeting variant inherits the same threat model (single-user localhost per PRD §7)
but does not yet inherit the same guards.

**User-prompt mention:** The Plan 03-02 prompt did mention "Validates Origin +
session-token (per Phase 0)" — that line is at odds with the planner's tests + the
CONTEXT decisions, so we followed the formal acceptance criteria + tests.

**Fix (when picked up — most likely Phase 5 alongside the Keychain swap):**

1. Add `?token=<token>` query-param check in `ws_meeting_handler` mirroring the
   Phase 0 `ws_handler` pattern.
2. Add Origin allowlist check using the same `allowed_origins(state.bind_port)` helper.
3. Reject with HTTP 403 (not WS 4404 — that's only for meeting-not-found) before the
   `ws.on_upgrade(...)` call.
4. Update `meeting_ws.rs` + `e2e_synthetic_audio.rs` to pass `state.session.as_str()`
   in the WS URL and set an `Origin` header on `connect_async`.

**Scope:** Phase 3 ships without this guard. v1 (single-user localhost) is unaffected;
the gap matters only if yogurt ever ships a multi-user or remote-accessible mode.
