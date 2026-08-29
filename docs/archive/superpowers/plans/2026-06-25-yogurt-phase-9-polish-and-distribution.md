# Yogurt v1 — Phase 9: Polish + Distribution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make yogurt v1 *shippable*. Every feature has been built in Phases 0-8; Phase 9 wraps them in a production-ready distribution. Three install channels (`brew install yogurt`, `cargo install yogurt`, direct tarball) all serve the same artifact produced by a tagged GitHub Actions release. A small but real polish pass — markdown export snapshot tests, README with screenshots and badges, crash recovery for orphaned meetings, a `yogurt doctor` diagnostic, SPDX headers, panic-on-abort — lands alongside the release pipeline. No telemetry, no notarization, no Sentry (per PRD §15: closed).

**Architecture:** A GitHub Actions matrix builds release tarballs for `aarch64-apple-darwin` and `x86_64-apple-darwin` on every `v*` tag push. Tarballs land on the GitHub Release; a second workflow opens a PR against a sibling `jarvisrchen/homebrew-yogurt` tap repo bumping the formula version + SHA256; a final job runs `cargo publish -p yogurt`. The release order is strict (artifacts → tap PR → cargo) to avoid race-conditions where the formula or the crate points at an artifact that doesn't exist yet. Local polish work — `yogurt doctor`, `yogurt --version`, orphaned-meeting recovery on startup, friendly crash page, panic = "abort" profile, optional self-update check (OFF by default) — slots into existing crates without restructuring.

**Tech Stack:** GitHub Actions · Homebrew (Ruby formula) · `cargo` 1.83+ · `cargo-dist` (evaluated, not used — see Task 9.0) · `insta = "1"` (snapshot tests for markdown export) · `clap` (subcommands `--version`, `doctor`) · `sysinfo` (system diagnostics in `doctor`) · `keyring` (already in use — `doctor` reads Keychain item presence, not values) · `tar` / `gzip` / `shasum` (release tarball + checksum) · `gh` CLI (manual release ops + tap repo bootstrap)

**Reference:** `docs/PRD.md` §11 (distribution & dev workflow), §14 (success criteria — all four must pass after this phase), §15 (license: MIT, repo: `github.com/jarvisrchen/yogurt`; no telemetry, no notarization).

**Out of scope (explicitly):**
- **No notarization.** Per PRD §15: deferred to v1.1. Documented in README with the right-click → Open workaround for macOS Gatekeeper.
- **No Sentry / crash telemetry / phone-home.** Per PRD §15: anti-goal. The optional self-update check is OFF by default and only ever performs a single unauthenticated GET to `api.github.com/repos/jarvisrchen/yogurt/releases/latest` — no payload, no identifiers, no opt-in flag flipped without explicit user consent in Settings.
- **No universal `lipo` binary.** Per-arch tarballs only — simpler, smaller per-arch downloads, no `lipo` step in CI. Decision documented in Task 9.5.
- **No new product features.** This phase only packages and polishes what Phases 0-8 already built. The crash-recovery flow uses local DB+markdown state only.
- **No Linux / Windows builds.** macOS-only per PRD §5.8.
- **No logic-bug fixes from prior phases.** If Phase 9 testing reveals a real bug from an earlier phase, file an issue and fix it under that phase's milestone — do **not** balloon Phase 9 to cover it.

---

## File structure produced by this phase

```
yogurt/
├── .github/
│   └── workflows/
│       ├── ci.yml                              # NEW · cargo test + clippy + fmt + pnpm test on every push
│       └── release.yml                         # NEW · tag v* → matrix build → tarball + tap PR + cargo publish
├── Cargo.toml                                  # MODIFY · description, homepage, documentation,
│                                               #          categories, keywords; panic = "abort"
├── CHANGELOG.md                                # NEW · v0.1.0 entry
├── CONTRIBUTING.md                             # NEW · dev setup, tests, workspace map, fmt + clippy
├── README.md                                   # MODIFY · install matrix, screenshots, badges, ascii arch
├── LICENSE                                     # VERIFY (added in Phase 0)
├── scripts/
│   ├── add-license-headers.sh                  # NEW · bash one-shot to add SPDX to every .rs file
│   └── release-checklist.md                    # NEW · manual checklist for cutting v0.1.0
├── crates/
│   ├── yogurt-cli/
│   │   ├── Cargo.toml                          # MODIFY · add sysinfo, keyring (re-export check only)
│   │   └── src/
│   │       ├── main.rs                         # MODIFY · register `doctor` subcommand
│   │       └── commands/
│   │           ├── mod.rs                      # MODIFY · pub mod doctor;
│   │           └── doctor.rs                   # NEW · diagnostics — rust, macOS, perms, providers, models
│   ├── yogurt-server/
│   │   ├── Cargo.toml                          # MODIFY · add insta (dev-dep), maybe `panic_hook`
│   │   └── src/
│   │       ├── lib.rs                          # MODIFY · install panic hook → friendly crash page
│   │       ├── crash.rs                        # NEW · panic hook + /__crash route (in-process snapshot)
│   │       ├── recovery.rs                     # NEW · on-boot orphan scan; offers DB+md restore
│   │       └── update_check.rs                 # NEW · optional GH releases poll (OFF by default)
│   ├── yogurt-db/
│   │   └── src/
│   │       └── migrations/
│   │           └── V004__add_recovery_marker.sql  # NEW (if Phase 7 didn't already add a heartbeat col)
│   └── yogurt-notes/
│       └── tests/
│           ├── markdown_export.rs              # NEW · insta snapshot tests for ~/.yogurt/notes/*.md
│           └── snapshots/                      # AUTO (insta writes .snap files here)
└── (sibling repo) jarvisrchen/homebrew-yogurt/
    ├── README.md                               # NEW (in sibling repo)
    └── Formula/
        └── yogurt.rb                           # NEW (in sibling repo) · formula template, auto-bumped
```

**Why this split:** the polish work touches every crate (CLI gets `doctor`, server gets crash + recovery + update-check, notes gets snapshot tests, db gets a tiny migration), but it doesn't *restructure* anything. The release pipeline lives entirely under `.github/workflows/`. The Homebrew tap lives in a sibling repo so the formula PR opened by CI doesn't pollute the main repo with bot commits — standard Homebrew convention.

---

## Test conventions reinforced in this phase

- **Snapshot tests for markdown export:** use `insta = "1"` with `assert_snapshot!`. Snapshot files live next to the test module under `crates/yogurt-notes/tests/snapshots/`. CI uses `INSTA_UPDATE=no` to fail on drift; local dev uses `cargo insta review`. (Established here — first snapshot tests in the workspace.)
- **CI matrix on every push:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm --dir web test`, `pnpm --dir web build`. Phases 0-8 ran these manually; Phase 9 makes them mandatory by enforcing them in `.github/workflows/ci.yml`.
- **Release pipeline dry-run:** before tagging `v0.1.0`, cut a `v0.0.99-rc1` test tag and verify all three artifact streams (tarballs, formula PR, cargo publish dry-run) produce clean output. The release workflow has a `dry-run: true` workflow_dispatch input that skips the actual publish steps.
- **No new E2E.** Playwright (added in Phase 7 or later) keeps its existing scope.

---

## Phase 9 task list

13 tasks. Each task ends with a commit. Approximate sequence: ~2-3 days of focused work. Task 9.0 is a decision task with no code commit; Task 9.13 is the final release dry-run + tag.

---

### Task 9.0 · Decision: hand-rolled release pipeline vs `cargo-dist`

**Files:** none — decision recorded in `docs/superpowers/plans/2026-06-25-yogurt-phase-9-polish-and-distribution.md` (this file) and `CHANGELOG.md` under "Release engineering".

- [ ] **Step 1: Read the `cargo-dist` 0.25 docs.**

Skim <https://opensource.axo.dev/cargo-dist/book/>. The relevant capabilities are:
- Generates GitHub Actions release workflow.
- Generates Homebrew formula and (optionally) opens the tap PR.
- Handles macOS universal binaries via `lipo`.
- Single source of truth in `[workspace.metadata.dist]`.

- [ ] **Step 2: Evaluate against this project's needs.**

Pro `cargo-dist`:
- Less hand-rolled YAML.
- Automatic SHA256SUMS + checksum verification.
- Free `installer.sh` script generation (curl-pipe install).

Con `cargo-dist`:
- Opaque magic — the generated workflow is ~600 lines and hard to debug.
- Imposes a versioning convention (one version across the workspace) that's *fine* but locks us in.
- Less control over the cargo-publish step ordering.
- Adds a third-party dependency on a 0.x tool to our release path.

- [ ] **Step 3: Make the call.**

**Decision: hand-roll the workflow.** Rationale:
1. The release surface is small (two targets, one binary crate, one formula). The hand-rolled workflow in Task 9.7 is ~150 lines — debuggable, no magic.
2. We need strict ordering control (artifacts → tap PR → `cargo publish`) to avoid the publish-hygiene race documented in this plan's appendix. `cargo-dist` does support custom ordering, but you write the YAML anyway.
3. Avoids tying release infra to a tool at 0.x — `cargo-dist` itself may break across versions.
4. We can always migrate to `cargo-dist` later — it can ingest existing setups.

Record the decision in `CHANGELOG.md` under the v0.1.0 entry's "Release engineering" subsection (added in Task 9.10).

- [ ] **Step 4: No commit — decision-only task.**

---

### Task 9.1 · `Cargo.toml` metadata for crates.io + `panic = "abort"`

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/yogurt-cli/Cargo.toml`

- [ ] **Step 1: Inspect current `Cargo.toml`.**

Run: `cat Cargo.toml`
Expected: workspace block from Phase 0 with `version = "0.1.0"`, `license = "MIT"`, `repository = "https://github.com/jarvisrchen/yogurt"`, and a `[profile.release]` block with `lto = "thin"`, `codegen-units = 1`, `strip = true`.

- [ ] **Step 2: Extend `[workspace.package]`.**

Append to the `[workspace.package]` table:

```toml
description = "Local-first meeting copilot — Granola's UX, your machine."
homepage = "https://github.com/jarvisrchen/yogurt"
documentation = "https://github.com/jarvisrchen/yogurt#readme"
readme = "README.md"
categories = ["command-line-utilities", "multimedia::audio"]
keywords = ["meeting", "transcription", "notes", "local-first", "granola"]
```

(crates.io requires the keys to be either in the binary crate's `[package]` or inherited from workspace via `description.workspace = true` etc. — the binary crate already inherits most of these; the new keys need explicit inheritance — see Step 3.)

- [ ] **Step 3: Inherit the new keys in `crates/yogurt-cli/Cargo.toml`.**

Append to the `[package]` table:

```toml
description.workspace = true
homepage.workspace = true
documentation.workspace = true
readme.workspace = true
categories.workspace = true
keywords.workspace = true
```

(`description` was already set in Phase 0; replace it with `.workspace = true`.)

- [ ] **Step 4: Add `panic = "abort"` to `[profile.release]`.**

Modify the existing `[profile.release]` block at the workspace root:

```toml
[profile.release]
lto = "thin"
codegen-units = 1
strip = true
panic = "abort"        # NEW · smaller binary, no unwinding tables, friendlier with panic hook → /__crash route
```

**Why abort?** Per PRD §15 (no telemetry), we don't ship a Sentry-style crash reporter. A panic during a meeting is unrecoverable from the user's perspective; the panic hook (Task 9.4) writes a marker so the recovery flow can pick it up on next start. Aborting also shrinks the binary by ~5-8% and matches `clap` + `tokio`'s recommended profile for CLIs.

- [ ] **Step 5: Verify the workspace still builds.**

Run: `cargo build --workspace --release`
Expected: clean build. Verify the resulting binary still runs: `./target/release/yogurt --help` shows the existing subcommands.

- [ ] **Step 6: Verify crates.io eligibility (without publishing).**

Run: `cargo publish -p yogurt --dry-run`
Expected: succeeds. The output will include "Verifying yogurt v0.1.0" and "Packaging" — no warnings about missing description / license / keywords. If warnings appear, fix them before continuing.

- [ ] **Step 7: Commit.**

```bash
git add Cargo.toml crates/yogurt-cli/Cargo.toml
git commit -m "chore: add crates.io metadata + panic = abort release profile"
```

---

### Task 9.2 · SPDX `// SPDX-License-Identifier: MIT` headers on every .rs file

**Files:**
- Create: `scripts/add-license-headers.sh`
- Modify: every `.rs` file in `crates/**/*.rs`

- [ ] **Step 1: Write `scripts/add-license-headers.sh`.**

```bash
#!/usr/bin/env bash
# Idempotent: adds `// SPDX-License-Identifier: MIT` as the first line of every
# .rs file under crates/, skipping files that already have it.
set -euo pipefail

HEADER="// SPDX-License-Identifier: MIT"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

added=0
skipped=0
while IFS= read -r -d '' file; do
    if head -n 1 "$file" | grep -qF "$HEADER"; then
        skipped=$((skipped + 1))
        continue
    fi
    # Prepend the header + a blank line.
    tmp="$(mktemp)"
    { printf '%s\n\n' "$HEADER"; cat "$file"; } > "$tmp"
    mv "$tmp" "$file"
    added=$((added + 1))
done < <(find "$ROOT/crates" -name '*.rs' -type f -print0)

echo "License headers: $added added, $skipped already present."
```

Make it executable: `chmod +x scripts/add-license-headers.sh`.

- [ ] **Step 2: Run it.**

```bash
./scripts/add-license-headers.sh
```

Expected output: `License headers: N added, 0 already present.` where N is the count of .rs files in the workspace. Re-run: `License headers: 0 added, N already present.` (idempotent).

- [ ] **Step 3: Verify build still passes.**

Run: `cargo build --workspace && cargo test --workspace`
Expected: clean. The SPDX line is a comment — it cannot break compilation but we verify because Step 2 touched dozens of files.

- [ ] **Step 4: Run `cargo fmt` to confirm the headers don't conflict with the formatter.**

Run: `cargo fmt --all --check`
Expected: no diff. If `fmt` wants to change anything, run `cargo fmt --all` and inspect — `rustfmt` should preserve the SPDX line at the top.

- [ ] **Step 5: Commit.**

```bash
git add scripts/add-license-headers.sh crates/
git commit -m "chore: add SPDX-License-Identifier: MIT header to every .rs file"
```

---

### Task 9.3 · `yogurt --version` and `yogurt doctor` subcommands

**Files:**
- Modify: `crates/yogurt-cli/Cargo.toml` (add `sysinfo`, optional `keyring` re-import)
- Modify: `crates/yogurt-cli/src/main.rs`
- Modify: `crates/yogurt-cli/src/commands/mod.rs`
- Create: `crates/yogurt-cli/src/commands/doctor.rs`
- Create: `crates/yogurt-cli/tests/doctor.rs`

- [ ] **Step 1: `--version` is already wired by clap.**

`clap` 4 with `#[command(name = "yogurt", version, ...)]` (set in Phase 0) automatically derives `--version` from `Cargo.toml`'s `version` field. Verify:

Run: `cargo run -p yogurt -- --version`
Expected: `yogurt 0.1.0`

No code change needed for `--version` itself. Move on to `doctor`.

- [ ] **Step 2: Add `sysinfo` to `crates/yogurt-cli/Cargo.toml`.**

Append to `[dependencies]`:

```toml
sysinfo = "0.32"
serde_json = { workspace = true }
```

(`serde_json` may already be present — keep one entry.)

- [ ] **Step 3: Write the failing `doctor` integration test.**

Create `crates/yogurt-cli/tests/doctor.rs`:

```rust
// SPDX-License-Identifier: MIT

use assert_cmd::Command;

#[test]
fn it_runs_doctor_and_prints_diagnostics() {
    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.arg("doctor");
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    // Diagnostic sections the user expects to see.
    for expected in [
        "yogurt doctor",
        "rust:",
        "macos:",
        "screen recording:",
        "db path:",
        "providers:",
        "stt:",
        "models:",
    ] {
        assert!(
            stdout.to_lowercase().contains(&expected.to_lowercase()),
            "doctor output missing section `{expected}`:\n{stdout}"
        );
    }
}

#[test]
fn it_runs_doctor_with_json_flag() {
    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.args(["doctor", "--json"]);
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .expect("--json output should parse as JSON");
    assert_eq!(v["service"], "yogurt-doctor");
    assert!(v["rust"].is_string());
    assert!(v["macos"].is_string());
}
```

- [ ] **Step 4: Run — expect compile failure (no `doctor` subcommand yet).**

Run: `cargo test -p yogurt --test doctor`
Expected: clap argument-parsing error or compile error referencing the missing `doctor` command.

- [ ] **Step 5: Register the subcommand in `crates/yogurt-cli/src/main.rs`.**

In the `Cmd` enum, add:

```rust
/// Print diagnostic info (rust version, macOS version, permissions, providers, models).
Doctor(DoctorArgs),
```

Define `DoctorArgs`:

```rust
#[derive(clap::Args, Debug)]
struct DoctorArgs {
    /// Emit diagnostics as JSON (for scripting / bug reports).
    #[arg(long)]
    json: bool,
}
```

In the `match cli.command` block, add:

```rust
Cmd::Doctor(args) => commands::doctor::run(commands::doctor::DoctorArgs {
    json: args.json,
}).await,
```

- [ ] **Step 6: Register the module in `crates/yogurt-cli/src/commands/mod.rs`.**

Append:

```rust
pub mod doctor;
```

- [ ] **Step 7: Write `crates/yogurt-cli/src/commands/doctor.rs`.**

```rust
// SPDX-License-Identifier: MIT

use anyhow::Result;
use serde_json::json;
use std::path::PathBuf;

pub struct DoctorArgs {
    pub json: bool,
}

pub async fn run(args: DoctorArgs) -> Result<()> {
    let report = collect().await;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report);
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct Report {
    service: &'static str,
    version: &'static str,
    rust: String,
    macos: String,
    screen_recording: String,    // "granted" | "denied" | "unknown"
    db_path: String,
    db_exists: bool,
    providers: Vec<String>,      // names only, NEVER keys
    stt: String,                 // "cloud:deepgram" | "local:whisper:small.en" | "unset"
    models: Vec<String>,         // whisper.cpp models on disk
}

async fn collect() -> Report {
    Report {
        service: "yogurt-doctor",
        version: env!("CARGO_PKG_VERSION"),
        rust: rust_version(),
        macos: macos_version(),
        screen_recording: screen_recording_status(),
        db_path: db_path().display().to_string(),
        db_exists: db_path().exists(),
        providers: list_providers(),
        stt: active_stt(),
        models: list_local_models(),
    }
}

fn rust_version() -> String {
    // Compiled-in via build script in Phase 0/9 — fallback to "unknown" if not present.
    option_env!("YOGURT_RUSTC_VERSION").unwrap_or("unknown").into()
}

fn macos_version() -> String {
    let mut sys = sysinfo::System::new();
    sys.refresh_all();
    sysinfo::System::os_version().unwrap_or_else(|| "unknown".into())
}

fn screen_recording_status() -> String {
    // The yogurt-audio crate exposes a `permission_status()` helper added in Phase 2.
    // If that crate isn't linked from the CLI yet (it shouldn't be, to keep doctor
    // light), shell out to a tiny probe — or simply report "unknown · check
    // System Settings → Privacy → Screen Recording".
    //
    // For now, return "unknown" + the bypass-hint. A later patch can wire to
    // yogurt-audio if there's user demand.
    "unknown (check System Settings → Privacy → Screen Recording)".into()
}

fn db_path() -> PathBuf {
    // Phase 7's yogurt-db crate sets the canonical path. Mirror it here without
    // a dep, to keep doctor's footprint small.
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".yogurt")
        .join("db.sqlite")
}

fn list_providers() -> Vec<String> {
    // Read ~/.yogurt/config.toml and list provider *names* only.
    let path = dirs::home_dir()
        .map(|h| h.join(".yogurt").join("config.toml"))
        .unwrap_or_default();
    let Ok(text) = std::fs::read_to_string(&path) else { return vec![] };
    // Minimal parse — full schema lives in yogurt-llm. For doctor, scrape `[providers.NAME]`.
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("[providers.").and_then(|s| s.strip_suffix("]")))
        .map(|s| s.to_string())
        .collect()
}

fn active_stt() -> String {
    // Same TOML scrape — look for `stt.kind = "cloud:deepgram"` etc.
    let path = dirs::home_dir()
        .map(|h| h.join(".yogurt").join("config.toml"))
        .unwrap_or_default();
    let Ok(text) = std::fs::read_to_string(&path) else { return "unset".into() };
    for line in text.lines() {
        if let Some(v) = line.trim().strip_prefix("kind") {
            return v.trim_start_matches(|c: char| c == '=' || c.is_whitespace())
                .trim_matches('"')
                .to_string();
        }
    }
    "unset".into()
}

fn list_local_models() -> Vec<String> {
    let dir = dirs::home_dir()
        .map(|h| h.join(".yogurt").join("models"))
        .unwrap_or_default();
    let Ok(read) = std::fs::read_dir(&dir) else { return vec![] };
    read.filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("ggml-") || n.ends_with(".bin"))
        .collect()
}

fn print_human(r: &Report) {
    println!("yogurt doctor — v{}\n", r.version);
    println!("  rust:              {}", r.rust);
    println!("  macOS:             {}", r.macos);
    println!("  screen recording:  {}", r.screen_recording);
    println!();
    println!("  db path:           {}", r.db_path);
    println!("  db exists:         {}", r.db_exists);
    println!();
    println!("  providers:         {}", if r.providers.is_empty() { "(none configured)".to_string() } else { r.providers.join(", ") });
    println!("  stt:               {}", r.stt);
    println!("  models:            {}", if r.models.is_empty() { "(none downloaded — using cloud STT?)".to_string() } else { r.models.join(", ") });
    println!();
    println!("  config:            {}", dirs::home_dir().map(|h| h.join(".yogurt/config.toml")).unwrap_or_default().display());
    println!("  notes:             {}", dirs::home_dir().map(|h| h.join(".yogurt/notes/")).unwrap_or_default().display());
    println!();
    println!("paste this output into any issue at https://github.com/jarvisrchen/yogurt/issues");
}
```

Add to `crates/yogurt-cli/Cargo.toml` `[dependencies]`:

```toml
dirs = "5"
serde = { workspace = true }
```

- [ ] **Step 8: Add a build script to capture `rustc --version`.**

Create `crates/yogurt-cli/build.rs`:

```rust
// SPDX-License-Identifier: MIT

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let rustc = Command::new("rustc").arg("--version").output();
    let v = rustc
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=YOGURT_RUSTC_VERSION={v}");
}
```

- [ ] **Step 9: Run the doctor tests — expect PASS.**

Run: `cargo test -p yogurt --test doctor`
Expected: 2 passing tests.

- [ ] **Step 10: Manual smoke.**

Run: `cargo run -p yogurt -- doctor`
Expected: ~15 lines of diagnostic output.

Run: `cargo run -p yogurt -- doctor --json | jq .`
Expected: valid JSON with the 10 fields.

Run: `cargo run -p yogurt -- --version`
Expected: `yogurt 0.1.0`.

- [ ] **Step 11: Commit.**

```bash
git add crates/yogurt-cli/
git commit -m "feat(cli): add yogurt doctor (diagnostics) + verify --version"
```

---

### Task 9.4 · Friendly crash page + panic hook

**Files:**
- Modify: `crates/yogurt-server/src/lib.rs`
- Create: `crates/yogurt-server/src/crash.rs`

- [ ] **Step 1: Write `crates/yogurt-server/src/crash.rs`.**

```rust
// SPDX-License-Identifier: MIT
//
// Crash handling. With `panic = "abort"` in the release profile, the process
// terminates on panic — no unwinding, no second-chance handler. The "friendly
// crash page" therefore has to be rendered *before* the panic actually triggers
// the abort. We do this with a custom panic hook that:
//   1. Writes a marker file to ~/.yogurt/last_crash.json (used by recovery.rs
//      on next start).
//   2. Logs a one-line summary to stderr.
//   3. Calls the default hook (which prints the panic + backtrace).
// The abort still happens — the user just gets a recoverable trail.
//
// Separately, /__crash is an in-process route that renders a friendly HTML page
// shown via a JS-side fetch wrapper: if any /api/* call returns 5xx OR the
// WebSocket drops mid-meeting, the frontend navigates to /__crash to show
// "yogurt crashed mid-meeting — restart and we'll recover your notes."

use axum::{response::Html, routing::get, Router};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
struct CrashMarker {
    when_unix_ms: u128,
    version: &'static str,
    panic_message: String,
    location: Option<String>,
    active_meeting_id: Option<String>,   // populated by recovery.rs via a shared atomic
}

pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let when_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let panic_message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<unknown>".into());
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line()));

        let marker = CrashMarker {
            when_unix_ms,
            version: env!("CARGO_PKG_VERSION"),
            panic_message,
            location,
            active_meeting_id: crate::recovery::current_meeting_id_snapshot(),
        };

        if let Some(path) = marker_path() {
            if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
            let _ = std::fs::write(&path, serde_json::to_string_pretty(&marker).unwrap_or_default());
        }

        eprintln!("\n[yogurt crash] {} — recovery marker written.", marker.panic_message);
        default(info);
    }));
}

fn marker_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".yogurt").join("last_crash.json"))
}

pub fn router() -> Router {
    Router::new().route("/__crash", get(crash_page))
}

async fn crash_page() -> Html<&'static str> {
    Html(CRASH_HTML)
}

const CRASH_HTML: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <title>yogurt — recovering</title>
    <style>
      body { background: #FBF7EF; color: #211D18; font-family: system-ui, -apple-system, sans-serif;
             max-width: 540px; margin: 12vh auto; padding: 32px; }
      h1 { font-family: 'Instrument Serif', Georgia, serif; font-weight: 400; font-size: 38px; margin: 0 0 12px; }
      p { font-size: 15px; line-height: 1.55; color: #4a4339; }
      code { background: #ECE9FB; padding: 2px 6px; border-radius: 4px; font-family: 'JetBrains Mono', ui-monospace, monospace; font-size: 13px; }
      .strawberry-dot { display: inline-block; width: 10px; height: 10px; background: #E07A66; border-radius: 50%; vertical-align: middle; margin-right: 8px; }
      .actions { margin-top: 28px; display: flex; gap: 12px; }
      a.btn { background: #5B4FC7; color: white; text-decoration: none; padding: 10px 18px; border-radius: 9px; font-weight: 600; font-size: 14px; }
      a.btn.outline { background: transparent; color: #211D18; border: 1px solid #D9D0C0; }
    </style>
  </head>
  <body>
    <h1><span class="strawberry-dot"></span>yogurt stopped unexpectedly</h1>
    <p>Your notes and transcript are safe — yogurt writes to <code>~/.yogurt/notes/*.md</code> as you type.</p>
    <p>Restart yogurt and the meeting you were in will be offered for recovery on the next library load. If nothing was recovered, the markdown file in <code>~/.yogurt/notes/</code> still has everything.</p>
    <p>Crash details: <code>~/.yogurt/last_crash.json</code> · paste it into <a href="https://github.com/jarvisrchen/yogurt/issues">an issue</a> if this keeps happening.</p>
    <div class="actions">
      <a class="btn" href="/">Open library</a>
      <a class="btn outline" href="https://github.com/jarvisrchen/yogurt/issues">File an issue</a>
    </div>
  </body>
</html>"#;
```

- [ ] **Step 2: Wire the hook + route into `crates/yogurt-server/src/lib.rs`.**

Add `mod crash;` near the other mods. In `pub async fn run`:

```rust
pub async fn run(addr: SocketAddr, mode: Mode) -> Result<()> {
    crash::install_panic_hook();
    let app = routes::router(mode).merge(crash::router());
    tracing::info!(?addr, ?mode, "yogurt-server starting");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 3: Add `dirs` + `serde_json` (already a workspace dep) to `crates/yogurt-server/Cargo.toml`.**

Append to `[dependencies]`:

```toml
dirs = "5"
serde = { workspace = true }
```

- [ ] **Step 4: Write a tiny smoke test for `/__crash`.**

Create `crates/yogurt-server/tests/crash.rs`:

```rust
// SPDX-License-Identifier: MIT

use std::time::Duration;

#[tokio::test]
async fn it_serves_crash_page() {
    let addr = "127.0.0.1:17882".parse().unwrap();
    let handle = tokio::spawn(async move {
        yogurt_server::run(addr, yogurt_server::Mode::Release).await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let body = reqwest::get("http://127.0.0.1:17882/__crash")
        .await.unwrap().text().await.unwrap();
    assert!(body.contains("yogurt stopped unexpectedly"));
    assert!(body.contains("/.yogurt/notes"));
    handle.abort();
}
```

- [ ] **Step 5: Run.**

Run: `cargo test -p yogurt-server`
Expected: all server tests pass, including the new `it_serves_crash_page`.

- [ ] **Step 6: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "feat(server): panic hook writes recovery marker + /__crash friendly page"
```

---

### Task 9.5 · Orphaned-meeting recovery on startup

**Files:**
- Create: `crates/yogurt-server/src/recovery.rs`
- Modify: `crates/yogurt-server/src/lib.rs` (call recovery on boot)
- Create: `crates/yogurt-db/src/migrations/V004__add_recovery_marker.sql` (only if Phase 7 didn't already add an end-time column)

- [ ] **Step 1: Inspect existing DB migrations.**

Run: `ls crates/yogurt-db/src/migrations/ 2>/dev/null || ls crates/yogurt-db/migrations/ 2>/dev/null`
Expected: V001-V003 from prior phases. If `meetings.ended_at` already exists, we use it directly. If not, V004 adds a tiny `is_orphaned BOOLEAN DEFAULT 0` flag.

(Assume `ended_at` exists per PRD §9. The recovery flow uses `ended_at IS NULL AND started_at < now() - 10 min` as the heuristic. If we discover during implementation that we need the explicit flag, the migration template is in Step 2.)

- [ ] **Step 2: (Conditional) Write `crates/yogurt-db/src/migrations/V004__add_recovery_marker.sql`.**

Only run this step if Phase 7 didn't add anything sufficient. Template:

```sql
-- V004: add explicit heartbeat to detect crashed-mid-recording meetings.
-- A row whose heartbeat_at is stale (>2 minutes) AND whose ended_at is NULL
-- is considered orphaned — the server crashed before /stop fired.
ALTER TABLE meetings ADD COLUMN heartbeat_at INTEGER;
CREATE INDEX idx_meetings_heartbeat ON meetings(heartbeat_at) WHERE ended_at IS NULL;
```

- [ ] **Step 3: Write `crates/yogurt-server/src/recovery.rs`.**

```rust
// SPDX-License-Identifier: MIT
//
// On server boot, scan for meetings whose ended_at IS NULL. Two scenarios:
//   1. The user was in a meeting and the server died (panic, kill -9, power loss).
//   2. The user closed the browser tab without ending — same DB state.
//
// For each orphan: confirm the markdown file at ~/.yogurt/notes/<slug>.md is
// present and non-empty, then mark the row with `recovered_at = now()`. The
// next library load surfaces a banner: "We recovered a meeting from your last
// session — open it." No automatic data mutation — just visibility.

use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

// A shared snapshot of the currently-recording meeting ID. Updated by the
// audio-pipeline glue (Phase 2) whenever a meeting starts/stops. Read by the
// panic hook so the crash marker has context.
static CURRENT_MEETING_ID: AtomicPtr<String> = AtomicPtr::new(std::ptr::null_mut());

pub fn set_current_meeting_id(id: Option<String>) {
    let new_ptr = match id {
        Some(s) => Box::into_raw(Box::new(s)),
        None => std::ptr::null_mut(),
    };
    let old = CURRENT_MEETING_ID.swap(new_ptr, Ordering::SeqCst);
    if !old.is_null() {
        unsafe { drop(Box::from_raw(old)); }
    }
}

pub fn current_meeting_id_snapshot() -> Option<String> {
    let ptr = CURRENT_MEETING_ID.load(Ordering::SeqCst);
    if ptr.is_null() { None } else {
        Some(unsafe { (*ptr).clone() })
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct OrphanedMeeting {
    pub id: String,
    pub title: String,
    pub started_at_unix_ms: i64,
    pub markdown_path: PathBuf,
    pub markdown_exists: bool,
    pub markdown_byte_size: u64,
}

/// Called once on server boot. Returns the list of meetings that were
/// in-progress at the last shutdown. The HTTP layer surfaces these via
/// `GET /api/meetings/orphaned`.
pub async fn scan_orphans(db: Arc<yogurt_db::Db>) -> Vec<OrphanedMeeting> {
    let rows = match db.list_orphaned_meetings().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(?e, "recovery: could not list orphaned meetings");
            return vec![];
        }
    };

    let notes_dir = dirs::home_dir()
        .map(|h| h.join(".yogurt").join("notes"))
        .unwrap_or_default();

    rows.into_iter()
        .map(|row| {
            // The slug convention is set in Phase 7 — mirror it here.
            let slug = slug_for(&row);
            let path = notes_dir.join(format!("{slug}.md"));
            let (exists, size) = match std::fs::metadata(&path) {
                Ok(m) => (true, m.len()),
                Err(_) => (false, 0),
            };
            OrphanedMeeting {
                id: row.id,
                title: row.title,
                started_at_unix_ms: row.started_at,
                markdown_path: path,
                markdown_exists: exists,
                markdown_byte_size: size,
            }
        })
        .collect()
}

fn slug_for(row: &yogurt_db::MeetingRow) -> String {
    // Match the Phase 7 convention: <YYYY-MM-DD-HHmm>-<slug-of-title>.
    // Bare-bones impl here for clarity — production uses the helper in
    // yogurt-notes::slugify.
    use chrono::{TimeZone, Utc};
    let dt = Utc.timestamp_millis_opt(row.started_at).single()
        .unwrap_or_else(Utc::now);
    let stamp = dt.format("%Y-%m-%d-%H%M");
    let title_slug: String = row.title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    format!("{stamp}-{title_slug}")
}
```

- [ ] **Step 4: Add a `list_orphaned_meetings` method to `yogurt-db`.**

In `crates/yogurt-db/src/lib.rs` (or wherever the `Db` impl lives), add:

```rust
pub async fn list_orphaned_meetings(&self) -> Result<Vec<MeetingRow>> {
    // Orphan = started_at < (now - 10 minutes) AND ended_at IS NULL.
    let cutoff_ms = chrono::Utc::now().timestamp_millis() - 10 * 60 * 1000;
    let rows = sqlx::query_as!(
        MeetingRow,
        "SELECT id, title, started_at, ended_at FROM meetings
         WHERE ended_at IS NULL AND started_at < ?
         ORDER BY started_at DESC",
        cutoff_ms,
    )
    .fetch_all(&self.pool)
    .await?;
    Ok(rows)
}
```

(If the DB layer uses `rusqlite` instead of `sqlx` per PRD §8 — adapt accordingly. The signature stays the same.)

- [ ] **Step 5: Wire the scan into `crates/yogurt-server/src/lib.rs`.**

```rust
pub async fn run(addr: SocketAddr, mode: Mode) -> Result<()> {
    crash::install_panic_hook();

    let db = yogurt_db::Db::open_default().await?;
    let orphans = recovery::scan_orphans(db.clone()).await;
    if !orphans.is_empty() {
        tracing::info!(count = orphans.len(), "recovery: orphaned meetings detected");
    }

    let app = routes::router(mode, db, orphans).merge(crash::router());
    // ... rest unchanged
}
```

(`routes::router` signature is extended in Step 6 to accept `Arc<Db>` + the orphan list.)

- [ ] **Step 6: Expose `GET /api/meetings/orphaned`.**

In `crates/yogurt-server/src/routes.rs`, register:

```rust
.route("/api/meetings/orphaned", get(orphaned))
```

Handler:

```rust
async fn orphaned(State(state): State<AppState>) -> Json<Vec<recovery::OrphanedMeeting>> {
    Json(state.orphans.clone())
}
```

(The `AppState` struct + `with_state` wiring is Phase 7 work — extend that struct with `orphans: Vec<OrphanedMeeting>`. If `AppState` doesn't exist yet, this task includes adding it; if it does, just add the field.)

- [ ] **Step 7: Write a focused integration test.**

Create `crates/yogurt-server/tests/recovery.rs`:

```rust
// SPDX-License-Identifier: MIT

// Smoke test: insert a row with ended_at = NULL and started_at = (now - 1h);
// verify GET /api/meetings/orphaned returns it. Uses a tmp DB.

// (Full setup omitted here; follow the pattern from Phase 7's API tests.)
```

(The full test wires a tmp `Db` instance and POSTs a fake meeting row — the implementation pattern is established in Phase 7; mirror it.)

- [ ] **Step 8: Add a frontend banner (the library already renders meetings).**

In `web/src/routes/Library.tsx` (Phase 7 artifact), add a small fetch + banner above the meeting list:

```tsx
const { data: orphans } = useQuery(["orphaned"], () =>
  fetch("/api/meetings/orphaned").then(r => r.json()),
);

return (
  <>
    {orphans?.length > 0 && (
      <div className="bg-blsoft border border-blue rounded-lg p-4 mb-6 flex items-center justify-between">
        <div>
          <div className="text-sm font-semibold text-ink">We recovered {orphans.length} meeting{orphans.length === 1 ? "" : "s"} from your last session</div>
          <div className="text-xs text-mut mt-1">Yogurt stopped unexpectedly. Your notes are safe.</div>
        </div>
        <a href={`/meeting/${orphans[0].id}`} className="text-sm text-blue font-medium">Open →</a>
      </div>
    )}
    {/* existing meeting list */}
  </>
);
```

- [ ] **Step 9: Manual smoke.**

1. Start yogurt, create a meeting, type some notes, then `kill -9` the binary mid-meeting (in another terminal: `pkill -9 yogurt`).
2. Restart `yogurt start`.
3. Visit `localhost:7878` — banner appears: "We recovered 1 meeting from your last session." Click "Open →" — the meeting loads with the notes intact.

- [ ] **Step 10: Commit.**

```bash
git add crates/yogurt-server/ crates/yogurt-db/ web/src/routes/Library.tsx
git commit -m "feat(server): recover orphaned meetings on boot + library banner"
```

---

### Task 9.6 · Snapshot tests for markdown export

**Files:**
- Modify: `crates/yogurt-notes/Cargo.toml` (add `insta` dev-dep)
- Create: `crates/yogurt-notes/tests/markdown_export.rs`
- Create: `crates/yogurt-notes/tests/snapshots/` (auto-populated by insta)

- [ ] **Step 1: Add `insta` to `crates/yogurt-notes/Cargo.toml`.**

Append to `[dev-dependencies]`:

```toml
insta = { version = "1", features = ["yaml"] }
```

- [ ] **Step 2: Write the snapshot test.**

Create `crates/yogurt-notes/tests/markdown_export.rs`:

```rust
// SPDX-License-Identifier: MIT

use yogurt_notes::{export_markdown, ExportInput, Transcript, TranscriptLine, Channel};

fn fixture_meeting() -> ExportInput {
    ExportInput {
        id: "01HRECJK7VABCDEFG".into(),
        title: "Weekly product sync".into(),
        started_at_unix_ms: 1_750_000_000_000,
        ended_at_unix_ms: Some(1_750_002_280_000),
        notes_md: "# Decisions\n- Ship the auth migration this sprint\n# Open questions\n- Storage limits for free tier?".into(),
        enriched_md: "# Decisions\n- Ship the auth migration this sprint\n- *Eng owner confirmed: Priya leads the rollout; staging behind feature flag.* ↳ 11:02\n# Open questions\n- Storage limits for free tier?\n- *Marketing wants 5 GB; engineering proposing 2 GB. Decision deferred to next week.* ↳ 23:14".into(),
        transcript: Transcript {
            lines: vec![
                TranscriptLine { ts_ms: 5_000,  channel: Channel::Mic,    text: "Let's kick off — anything blocking?".into() },
                TranscriptLine { ts_ms: 12_000, channel: Channel::System, text: "Auth migration is the big one this sprint.".into() },
                TranscriptLine { ts_ms: 662_000, channel: Channel::Mic,   text: "Priya, you want to lead the rollout?".into() },
                TranscriptLine { ts_ms: 674_000, channel: Channel::System, text: "Yeah, staging behind a flag first.".into() },
                TranscriptLine { ts_ms: 1_394_000, channel: Channel::Mic, text: "What about storage limits for free tier?".into() },
                TranscriptLine { ts_ms: 1_406_000, channel: Channel::System, text: "Marketing wants 5 GB, eng is proposing 2 GB.".into() },
            ],
        },
    }
}

#[test]
fn it_exports_a_full_meeting_to_markdown() {
    let input = fixture_meeting();
    let md = export_markdown(&input);
    insta::assert_snapshot!("full_meeting", md);
}

#[test]
fn it_yields_valid_yaml_front_matter() {
    let input = fixture_meeting();
    let md = export_markdown(&input);

    // Front-matter sits between two `---` fences at the top.
    let parts: Vec<&str> = md.splitn(3, "---\n").collect();
    assert!(parts.len() >= 3, "expected `---\\n...\\n---\\n` front-matter block, got:\n{md}");
    let yaml = parts[1];
    let parsed: serde_yaml::Value = serde_yaml::from_str(yaml)
        .expect("front-matter should be valid YAML");

    assert_eq!(parsed["id"].as_str(), Some("01HRECJK7VABCDEFG"));
    assert_eq!(parsed["title"].as_str(), Some("Weekly product sync"));
}

#[test]
fn it_renders_deep_links_in_HHMM_form() {
    let input = fixture_meeting();
    let md = export_markdown(&input);
    // The enriched bullets in the fixture end with `↳ 11:02` and `↳ 23:14`.
    assert!(md.contains("↳ 11:02"), "missing deep link to 11:02");
    assert!(md.contains("↳ 23:14"), "missing deep link to 23:14");
    // Verify no malformed `↳ HH:MM:SS` or `↳ MM:SS` slipped through.
    let bad = regex::Regex::new(r"↳ \d+:\d+:\d+").unwrap();
    assert!(!bad.is_match(&md), "deep links must be HH:MM only, no seconds:\n{md}");
}

#[test]
fn it_appends_transcript_with_channel_labels() {
    let input = fixture_meeting();
    let md = export_markdown(&input);

    let appendix_start = md.find("## Transcript").expect("transcript appendix");
    let appendix = &md[appendix_start..];

    assert!(appendix.contains("**Me**") || appendix.contains("Me ·"), "should label mic channel as 'Me'");
    assert!(appendix.contains("**Them**") || appendix.contains("Them ·"), "should label system channel as 'Them'");
    assert!(appendix.contains("00:00:05"), "transcript timestamps should be HH:MM:SS");
    assert!(appendix.contains("23:14:00") || appendix.contains("23:26:00"), "longer timestamps should still parse");
}
```

(Add `serde_yaml`, `regex` to `[dev-dependencies]` if they aren't already pulled transitively.)

- [ ] **Step 3: Run — first time, snapshot is missing → insta writes a `.snap.new` file.**

Run: `cargo test -p yogurt-notes --test markdown_export`
Expected: 3 tests pass, 1 test fails with `INSTA_UPDATE` hint (the first snapshot run is always pending).

- [ ] **Step 4: Review and accept the snapshot.**

```bash
cargo install cargo-insta   # one-time
cargo insta review -p yogurt-notes
```

Press `a` to accept the generated snapshot. This writes `crates/yogurt-notes/tests/snapshots/markdown_export__full_meeting.snap`.

Inspect the .snap file by hand — verify it has front-matter, the merged notes block, and the appendix in the expected order.

- [ ] **Step 5: Rerun — expect all 4 tests pass.**

Run: `cargo test -p yogurt-notes --test markdown_export`
Expected: 4 / 4.

- [ ] **Step 6: CI guardrail.**

The CI workflow (Task 9.8) sets `INSTA_UPDATE=no`. Any drift in export output will fail CI — the dev workflow on a real change is `cargo insta review` + commit the updated snapshot.

- [ ] **Step 7: Commit.**

```bash
git add crates/yogurt-notes/
git commit -m "test(notes): insta snapshot tests for markdown export — front-matter, deep links, transcript"
```

---

### Task 9.7 · Optional self-update check (OFF by default)

**Files:**
- Create: `crates/yogurt-server/src/update_check.rs`
- Modify: `crates/yogurt-server/src/lib.rs` (call once at boot if enabled)

- [ ] **Step 1: Design constraint reminder.**

Per PRD §15 (closed): **no phone-home of any kind in v1.** Self-update check is opt-in via Settings; OFF by default; if enabled, performs *one* unauthenticated GET to `api.github.com/repos/jarvisrchen/yogurt/releases/latest` per server boot; sends no payload, no identifiers, no User-Agent beyond `yogurt/${version}`.

If the user has not flipped the toggle, the code path never executes and no network call is made.

- [ ] **Step 2: Write `crates/yogurt-server/src/update_check.rs`.**

```rust
// SPDX-License-Identifier: MIT

use serde::Deserialize;

#[derive(Deserialize)]
struct Release {
    tag_name: String,        // e.g. "v0.2.0"
    html_url: String,
}

const URL: &str = "https://api.github.com/repos/jarvisrchen/yogurt/releases/latest";

/// Called once at boot, only if `settings.update_check_enabled == true`.
/// Returns Some(message) if a newer version is available, else None.
/// Never panics on network errors — silently returns None.
pub async fn check_for_update() -> Option<String> {
    let client = reqwest::Client::builder()
        .user_agent(format!("yogurt/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(5))
        .build().ok()?;
    let release: Release = client.get(URL).send().await.ok()?.json().await.ok()?;
    let latest = release.tag_name.trim_start_matches('v');
    let current = env!("CARGO_PKG_VERSION");
    if semver_gt(latest, current) {
        Some(format!("yogurt {latest} is available — {}", release.html_url))
    } else {
        None
    }
}

fn semver_gt(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Option<(u32, u32, u32)> {
        let mut parts = s.split('.').map(|p| p.parse::<u32>().ok());
        Some((parts.next()??, parts.next()??, parts.next()??))
    };
    match (parse(a), parse(b)) {
        (Some(x), Some(y)) => x > y,
        _ => false,
    }
}
```

- [ ] **Step 3: Wire into boot with a settings check.**

In `crates/yogurt-server/src/lib.rs`, after recovery:

```rust
let settings = yogurt_db::settings::load(&db).await?;
if settings.update_check_enabled {
    let result = update_check::check_for_update().await;
    if let Some(msg) = result {
        tracing::info!(%msg, "update available");
        // Surface to frontend via GET /api/system/update-available
    }
}
```

(`yogurt_db::settings` is a Phase 5 artifact — extend the settings struct with `update_check_enabled: bool` default `false`.)

- [ ] **Step 4: Add the toggle to the Settings UI.**

In `web/src/routes/Settings.tsx` (Phase 5 artifact), in the General section, append:

```tsx
<Row>
  <Label>Check for updates on startup</Label>
  <Toggle
    checked={settings.update_check_enabled}
    onChange={(v) => updateSettings({ update_check_enabled: v })}
  />
  <Caption>
    One unauthenticated request to api.github.com. No data sent. Off by default.
  </Caption>
</Row>
```

- [ ] **Step 5: Manual smoke (toggle off → no network call).**

Run: `yogurt start` with the toggle off. Open Wireshark / `lsof -i` to verify no connection to `api.github.com`. Toggle on, restart — one request appears, no others.

- [ ] **Step 6: Commit.**

```bash
git add crates/yogurt-server/src/update_check.rs crates/yogurt-server/src/lib.rs web/src/routes/Settings.tsx crates/yogurt-db/
git commit -m "feat(server): optional self-update check (off by default per anti-telemetry stance)"
```

---

### Task 9.8 · `.github/workflows/ci.yml` — fmt + clippy + test on every push

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Write `.github/workflows/ci.yml`.**

```yaml
name: ci

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1
  INSTA_UPDATE: no            # snapshot drift = failure

jobs:
  rust:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: "1.83"
          components: rustfmt, clippy
      - name: Cargo cache
        uses: Swatinem/rust-cache@v2
      - name: Install pnpm
        uses: pnpm/action-setup@v4
        with:
          version: 9
      - name: pnpm install
        run: pnpm --dir web install --frozen-lockfile
      - name: pnpm build (needed for rust-embed)
        run: pnpm --dir web build
      - name: cargo fmt
        run: cargo fmt --all -- --check
      - name: cargo clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: cargo test
        run: cargo test --workspace --no-fail-fast

  web:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: "20"
          cache: "pnpm"
          cache-dependency-path: web/pnpm-lock.yaml
      - run: pnpm --dir web install --frozen-lockfile
      - run: pnpm --dir web test
      - run: pnpm --dir web build
```

- [ ] **Step 2: Commit + push to a feature branch.**

```bash
git checkout -b ci/setup
git add .github/workflows/ci.yml
git commit -m "ci: add cargo fmt + clippy + test + pnpm test on every push"
git push -u origin ci/setup
```

- [ ] **Step 3: Open a PR to `main` and watch CI.**

```bash
gh pr create --title "ci: add cargo + pnpm pipeline" --body "Phase 9 task 9.8"
gh pr checks --watch
```

Expected: both jobs (`rust`, `web`) green. If clippy fails on something Phase 0-8 introduced, fix the lint here (this is part of the polish — Phase 9 is the first phase where clippy is gated). If a real bug is uncovered, file an issue under that phase, fix it, push.

- [ ] **Step 4: Merge once green.**

```bash
gh pr merge --squash --delete-branch
git checkout main && git pull
```

---

### Task 9.9 · `.github/workflows/release.yml` — tag v* → tarballs + tap PR + cargo publish

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Plan the workflow in plain English.**

Triggers on push of any tag matching `v*` (or `workflow_dispatch` with a `dry-run: true` input for testing).

Three jobs, strictly ordered:

1. **`build` (matrix)**: builds `aarch64-apple-darwin` and `x86_64-apple-darwin` tarballs in parallel. Each runs `pnpm --dir web build` first (frontend), then `cargo build --release --target ${target}`. Strips the binary, tarballs it, computes SHA256.
2. **`release` (depends on `build`)**: downloads both tarballs, generates `SHA256SUMS`, creates the GitHub Release, uploads all artifacts. **Stops here if `dry-run: true`.**
3. **`tap` (depends on `release`)**: clones `jarvisrchen/homebrew-yogurt`, updates `Formula/yogurt.rb` with the new version + per-arch SHAs, opens a PR. Skipped on dry-run.
4. **`publish` (depends on `release` AND `tap`)**: runs `cargo publish -p yogurt`. Skipped on dry-run; idempotent (skips if the version is already on crates.io).

**Why this order:** if `cargo publish` runs *before* the GitHub Release exists, `cargo install yogurt` works but `brew install yogurt` 404s on the tarball download. We block `tap` and `publish` on the release being live.

- [ ] **Step 2: Write the workflow.**

```yaml
name: release

on:
  push:
    tags: ["v*"]
  workflow_dispatch:
    inputs:
      dry-run:
        description: "Run all build steps but skip the actual publish/PR jobs."
        type: boolean
        default: true

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  build:
    name: build · ${{ matrix.target }}
    runs-on: macos-latest
    strategy:
      fail-fast: false
      matrix:
        target:
          - aarch64-apple-darwin
          - x86_64-apple-darwin
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust + target
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: "1.83"
          targets: ${{ matrix.target }}

      - uses: Swatinem/rust-cache@v2
        with:
          key: release-${{ matrix.target }}

      - uses: pnpm/action-setup@v4
        with: { version: 9 }
      - uses: actions/setup-node@v4
        with:
          node-version: "20"
          cache: "pnpm"
          cache-dependency-path: web/pnpm-lock.yaml

      - name: pnpm build (frontend → web/dist for rust-embed)
        run: pnpm --dir web install --frozen-lockfile && pnpm --dir web build

      - name: cargo build --release
        run: cargo build --release --target ${{ matrix.target }} -p yogurt

      - name: Strip binary
        run: strip target/${{ matrix.target }}/release/yogurt

      - name: Tarball
        run: |
          set -euo pipefail
          NAME="yogurt-${{ matrix.target }}.tar.gz"
          tar -czf "$NAME" -C target/${{ matrix.target }}/release yogurt
          shasum -a 256 "$NAME" > "$NAME.sha256"
          ls -la "$NAME" "$NAME.sha256"

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: yogurt-${{ matrix.target }}
          path: |
            yogurt-${{ matrix.target }}.tar.gz
            yogurt-${{ matrix.target }}.tar.gz.sha256

  release:
    name: github release
    needs: build
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4

      - uses: actions/download-artifact@v4
        with:
          pattern: yogurt-*
          merge-multiple: true

      - name: Combine SHA256SUMS
        run: cat yogurt-*.tar.gz.sha256 > SHA256SUMS

      - name: Resolve version
        id: ver
        run: |
          if [ "${{ github.event_name }}" = "push" ]; then
            VER="${GITHUB_REF#refs/tags/v}"
          else
            VER="0.0.99-rc-dryrun-$(date +%s)"
          fi
          echo "version=$VER" >> "$GITHUB_OUTPUT"

      - name: Create GitHub Release
        if: github.event_name == 'push' || inputs.dry-run == false
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ github.ref_name }}
          name: yogurt v${{ steps.ver.outputs.version }}
          body_path: ./CHANGELOG.md
          files: |
            yogurt-aarch64-apple-darwin.tar.gz
            yogurt-x86_64-apple-darwin.tar.gz
            SHA256SUMS

      - name: Echo (dry-run mode)
        if: inputs.dry-run == true
        run: |
          echo "DRY RUN — would upload these files to GH Release v${{ steps.ver.outputs.version }}:"
          ls -la yogurt-*.tar.gz SHA256SUMS

  tap:
    name: homebrew tap PR
    needs: release
    if: github.event_name == 'push'   # never opens PRs on dry-run
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
        with:
          pattern: yogurt-*
          merge-multiple: true

      - name: Compute SHAs
        id: shas
        run: |
          ARM_SHA=$(shasum -a 256 yogurt-aarch64-apple-darwin.tar.gz | awk '{print $1}')
          X86_SHA=$(shasum -a 256 yogurt-x86_64-apple-darwin.tar.gz | awk '{print $1}')
          echo "arm_sha=$ARM_SHA" >> "$GITHUB_OUTPUT"
          echo "x86_sha=$X86_SHA" >> "$GITHUB_OUTPUT"

      - name: Resolve version
        id: ver
        run: echo "version=${GITHUB_REF#refs/tags/v}" >> "$GITHUB_OUTPUT"

      - name: Checkout tap
        uses: actions/checkout@v4
        with:
          repository: jarvisrchen/homebrew-yogurt
          path: tap
          token: ${{ secrets.HOMEBREW_TAP_TOKEN }}   # PAT with repo scope on the tap

      - name: Rewrite formula
        working-directory: tap
        run: |
          cat > Formula/yogurt.rb <<EOF
          class Yogurt < Formula
            desc "Local-first meeting copilot — Granola's UX, your machine."
            homepage "https://github.com/jarvisrchen/yogurt"
            version "${{ steps.ver.outputs.version }}"
            license "MIT"

            on_macos do
              if Hardware::CPU.arm?
                url "https://github.com/jarvisrchen/yogurt/releases/download/v#{version}/yogurt-aarch64-apple-darwin.tar.gz"
                sha256 "${{ steps.shas.outputs.arm_sha }}"
              else
                url "https://github.com/jarvisrchen/yogurt/releases/download/v#{version}/yogurt-x86_64-apple-darwin.tar.gz"
                sha256 "${{ steps.shas.outputs.x86_sha }}"
              end
            end

            def install
              bin.install "yogurt"
            end

            test do
              assert_match "yogurt", shell_output("#{bin}/yogurt --version")
            end
          end
          EOF

      - name: Open PR
        working-directory: tap
        env:
          GH_TOKEN: ${{ secrets.HOMEBREW_TAP_TOKEN }}
        run: |
          git config user.name "yogurt-release-bot"
          git config user.email "release-bot@yogurt.local"
          BRANCH="bump-${{ steps.ver.outputs.version }}"
          git checkout -b "$BRANCH"
          git add Formula/yogurt.rb
          git commit -m "yogurt ${{ steps.ver.outputs.version }}"
          git push origin "$BRANCH"
          gh pr create --title "yogurt ${{ steps.ver.outputs.version }}" \
                       --body "Auto-PR from the yogurt release workflow." \
                       --base main --head "$BRANCH"

  publish:
    name: cargo publish
    needs: [release, tap]
    if: github.event_name == 'push'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { toolchain: "1.83" }
      - name: Publish (skip if already on crates.io)
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CRATES_IO_TOKEN }}
        run: |
          set -e
          VER="${GITHUB_REF#refs/tags/v}"
          # Idempotency: if a build with this version is already on crates.io, exit 0.
          if cargo search yogurt | grep -q "^yogurt = \"$VER\""; then
            echo "yogurt $VER already published — skipping."
            exit 0
          fi
          cargo publish -p yogurt
```

- [ ] **Step 3: Add the two repository secrets via `gh`.**

```bash
gh secret set CRATES_IO_TOKEN < ~/.cargo/credentials.toml-token   # paste the token
gh secret set HOMEBREW_TAP_TOKEN                                   # paste a PAT with repo scope on jarvisrchen/homebrew-yogurt
```

(The PAT must have `repo` scope on the tap repo, no more. Per anti-telemetry stance: no GitHub App, no fine-grained team token.)

- [ ] **Step 4: Commit.**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add release workflow — matrix build + GH Release + tap PR + cargo publish"
```

(We do not push a tag yet — that's Task 9.13's dry-run + the manual v0.1.0 cut.)

---

### Task 9.10 · Bootstrap the `jarvisrchen/homebrew-yogurt` tap repo

**Files:** all in a sibling repo: `jarvisrchen/homebrew-yogurt/README.md`, `Formula/yogurt.rb`.

- [ ] **Step 1: Create the repo.**

```bash
gh repo create jarvisrchen/homebrew-yogurt \
  --public \
  --description "Homebrew tap for yogurt — local-first meeting copilot." \
  --license=MIT
```

- [ ] **Step 2: Bootstrap the README in the new repo.**

```bash
cd /tmp
git clone https://github.com/jarvisrchen/homebrew-yogurt.git
cd homebrew-yogurt
mkdir -p Formula
```

Create `README.md`:

```markdown
# homebrew-yogurt

Homebrew tap for [yogurt](https://github.com/jarvisrchen/yogurt) — a local-first, open-source meeting copilot.

## Install

```bash
brew install jarvisrchen/yogurt/yogurt
# or, after tap:
brew tap jarvisrchen/yogurt
brew install yogurt
```

## License

MIT
```

- [ ] **Step 3: Add a placeholder formula.**

Create `Formula/yogurt.rb` (the release workflow rewrites this on every tag; the initial commit is a stub so the formula path exists):

```ruby
class Yogurt < Formula
  desc "Local-first meeting copilot — Granola's UX, your machine."
  homepage "https://github.com/jarvisrchen/yogurt"
  version "0.0.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/jarvisrchen/yogurt/releases/download/v0.0.0/yogurt-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    else
      url "https://github.com/jarvisrchen/yogurt/releases/download/v0.0.0/yogurt-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "yogurt"
  end

  test do
    assert_match "yogurt", shell_output("#{bin}/yogurt --version")
  end
end
```

- [ ] **Step 4: Push + verify.**

```bash
git add README.md Formula/yogurt.rb
git commit -m "init: placeholder formula (release workflow auto-bumps on tag)"
git push origin main
```

- [ ] **Step 5: Manually verify the PAT works.**

```bash
# From inside the tap repo:
gh auth status
# Create a throwaway branch + PR and immediately close, to confirm the PAT in
# Task 9.9 Step 3 has the right permissions.
```

- [ ] **Step 6: Return to the yogurt repo.**

```bash
cd /Users/rchen/Documents/code/yogurt
```

No commit in the main repo for this task — the work lives in the sibling repo.

---

### Task 9.11 · `CHANGELOG.md` for v0.1.0

**Files:**
- Create: `CHANGELOG.md`

- [ ] **Step 1: Write `CHANGELOG.md`.**

```markdown
# Changelog

All notable changes to yogurt are documented here. Format loosely follows [Keep a Changelog](https://keepachangelog.com/).

## [0.1.0] — 2026-07-XX

**The first public release.** Yogurt v1 ships a local-first, open-source meeting copilot for macOS — Granola's augmented-notes UX, your audio never leaves the machine unless you opt into a cloud STT provider.

### Added
- **System + mic audio capture** via ScreenCaptureKit (no meeting-bot, no kext, no BlackHole). Phase 2.
- **Pluggable transcription**: Deepgram cloud (default) and whisper.cpp local (privacy mode). Phases 3, 8.
- **Augmented markdown notes** — black user content, grey AI-added content, deep-links from each AI bullet to the transcript timestamp it came from. Phase 4.
- **In-meeting AI chat** — floating `⌘K` "Ask this meeting…" pill, streams transcript-aware Q&A through any OpenAI-compatible LLM. Phase 6.
- **Settings UI** with provider preset cards (Minimax, Ollama, LM Studio, OpenRouter, OpenAI), masked Keychain storage, side-by-side cloud / local STT picker. Phase 5.
- **Meeting library** with date-grouped cards, folders, search, and a "Local-only · on" status pill. Phase 7.
- **Onboarding flow** — 3-step screen-recording / model / transcription setup. Phase 7.
- **Friendly empty + error states** including a "permission not granted" recovery card. Phase 7.
- **Local STT** via whisper.cpp on Metal with first-run model-download UX. Phase 8.
- **Markdown export** — every meeting writes to `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>.md` (front-matter + enriched notes + transcript appendix). Phase 7.
- **`yogurt doctor`** subcommand — diagnostic dump (Rust, macOS, permissions, providers, models, db path) for bug reports. Phase 9.
- **Crash recovery** — orphaned meetings detected on next start, library shows a recovery banner. Notes are always safe in the markdown file. Phase 9.
- **Friendly crash page** at `/__crash` — no Sentry, no telemetry, just a recoverable message + a link to file an issue. Phase 9.
- **Optional self-update check** — single GET to `api.github.com/repos/jarvisrchen/yogurt/releases/latest`, OFF by default. Phase 9.
- **Three install channels**: `brew install yogurt` (Homebrew tap), `cargo install yogurt` (crates.io), direct download from the GitHub Release. Phase 9.

### Release engineering

- Hand-rolled GitHub Actions release workflow (decision recorded in Phase 9 plan Task 9.0 — `cargo-dist` evaluated, not adopted).
- Per-architecture macOS tarballs (`aarch64-apple-darwin`, `x86_64-apple-darwin`). No universal `lipo` binary — simpler, smaller downloads per arch.
- Tag-driven release: `git tag v0.1.0 && git push --tags` produces tarballs + opens a tap PR + publishes to crates.io.
- **No code signing or notarization** — deferred to v1.1. On first launch, macOS may show "unverified developer"; users right-click → Open to bypass (documented in README).

### Privacy

- No telemetry, no Sentry, no crash reporting service. Crash markers stay local at `~/.yogurt/last_crash.json`.
- No phone-home of any kind unless the user explicitly enables the self-update check.
- Audio deleted immediately after transcription.
- API keys stored in macOS Keychain via `keyring` — never in `~/.yogurt/config.toml`.

### Known limitations

- macOS 13+ only.
- Apple Silicon is the primary target; Intel works but `whisper.cpp` is limited to `small.en` at real-time.
- First launch after `brew install` triggers a macOS Gatekeeper warning — see README for the right-click → Open workaround.
- Re-enhance regenerates with the same bundled prompt; template picker is v2 (per PRD §6 item 2).

### Acknowledgements

- Inspired by [Granola.ai](https://www.granola.ai/).
- Built on the shoulders of [whisper.cpp](https://github.com/ggerganov/whisper.cpp), [axum](https://github.com/tokio-rs/axum), [TipTap](https://tiptap.dev/), [rust-embed](https://github.com/pyrossh/rust-embed), and the macOS [ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit) framework.

[0.1.0]: https://github.com/jarvisrchen/yogurt/releases/tag/v0.1.0
```

- [ ] **Step 2: Commit.**

```bash
git add CHANGELOG.md
git commit -m "docs: add CHANGELOG.md with v0.1.0 entry"
```

---

### Task 9.12 · `README.md` polish + `CONTRIBUTING.md`

**Files:**
- Modify: `README.md`
- Create: `CONTRIBUTING.md`

- [ ] **Step 1: Find screenshots for embedding.**

Check `yogurt-app-design/project/screenshots/` for any rendered design boards.

```bash
ls yogurt-app-design/project/screenshots/ 2>/dev/null || echo "no design screenshots — use placeholder paths and fill in once real ones exist"
```

If screenshots exist, copy 3-4 into a new `docs/screenshots/` directory (committed) — meeting view, library view, settings view, onboarding view.

```bash
mkdir -p docs/screenshots
# cp yogurt-app-design/project/screenshots/{library,meeting,settings,onboarding}.png docs/screenshots/
```

- [ ] **Step 2: Rewrite `README.md`.**

```markdown
# yogurt

> Local-first, open-source meeting copilot for macOS. Granola's augmented-notes UX, your machine.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/yogurt.svg)](https://crates.io/crates/yogurt)
[![GitHub Release](https://img.shields.io/github/v/release/jarvisrchen/yogurt)](https://github.com/jarvisrchen/yogurt/releases)
[![CI](https://github.com/jarvisrchen/yogurt/actions/workflows/ci.yml/badge.svg)](https://github.com/jarvisrchen/yogurt/actions/workflows/ci.yml)

![Meeting view](docs/screenshots/meeting.png)

Yogurt captures both your mic and your Mac's system audio without joining the call as a bot, transcribes live, and produces *augmented notes* — your sparse markdown notes fused in-place with what was actually said. Your text stays black; AI-added text renders grey; clicking an AI bullet opens the transcript at the moment it came from. Bring your own OpenAI-compatible LLM key.

## Install

### Homebrew (recommended for non-developers)

```bash
brew install jarvisrchen/yogurt/yogurt
yogurt start
```

### Cargo

```bash
cargo install yogurt
yogurt start
```

### Direct download

Grab the matching tarball from the [latest release](https://github.com/jarvisrchen/yogurt/releases/latest):

```bash
# Apple Silicon
curl -L https://github.com/jarvisrchen/yogurt/releases/latest/download/yogurt-aarch64-apple-darwin.tar.gz | tar xz
./yogurt start

# Intel
curl -L https://github.com/jarvisrchen/yogurt/releases/latest/download/yogurt-x86_64-apple-darwin.tar.gz | tar xz
./yogurt start
```

### First-launch on macOS

Yogurt is **not notarized** in v1 (deferred to v1.1 per [PRD §15](docs/PRD.md#15-open-questions-for-future-rounds)). On first launch, macOS may show:

> "yogurt" can't be opened because Apple cannot check it for malicious software.

To bypass: **right-click the binary → Open → Open** (or `xattr -d com.apple.quarantine /opt/homebrew/bin/yogurt` once). You only have to do this once per install. The source code is on GitHub and the release binaries are built by [our public CI](.github/workflows/release.yml) — feel free to inspect or build from source.

## First-run

1. `yogurt start` — opens `http://localhost:7878` in your default browser.
2. macOS prompts for Screen Recording permission (this is how yogurt hears the other side of the call — no meeting bot).
3. Settings → Model → paste any OpenAI-compatible base URL + API key (Minimax, OpenAI, OpenRouter, Ollama, LM Studio, ...).
4. New meeting → record → type sparse bullets → End → augmented notes appear.

![Library view](docs/screenshots/library.png)

## Architecture (one diagram)

```
┌────────────┐     HTTP+WS     ┌────────────────────────────────┐
│  Browser   │ ←──────────────→│       yogurt (one binary)       │
│ (Chrome /  │  localhost:7878 │                                 │
│  Safari)   │                 │  axum · audio · STT · LLM · DB  │
└────────────┘                 └──────────────┬──────────────────┘
                                              │
                                              ▼ (only if cloud STT/LLM chosen)
                                  ┌──────────────────────┐
                                  │ Deepgram · Minimax · │
                                  │ OpenAI · OpenRouter  │
                                  │ Ollama · ...         │
                                  └──────────────────────┘
```

Single static Rust binary; web UI embedded via `rust-embed`. Detailed architecture in [docs/PRD.md §7](docs/PRD.md#7-architecture).

## Run from source

```bash
brew install rust pnpm
git clone https://github.com/jarvisrchen/yogurt.git
cd yogurt
pnpm --dir web install

# dev — two terminals
pnpm --dir web dev                              # frontend HMR on :5173
cargo run -p yogurt -- start --dev              # backend on :7878

# release build
pnpm --dir web build
cargo build --release
./target/release/yogurt start
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the dev environment, test commands, and workspace layout.

## Diagnostics

```bash
yogurt doctor              # human-readable diagnostic dump
yogurt doctor --json       # machine-readable, paste into bug reports
yogurt --version
```

## Privacy posture

- **Audio never leaves your machine** unless you choose a cloud STT provider (Deepgram / AssemblyAI / Groq). The default settings panel makes this contract explicit.
- **No phone-home, no telemetry, no Sentry.** Per [PRD §15](docs/PRD.md#15-open-questions-for-future-rounds), the only outbound request yogurt ever makes (other than user-chosen STT/LLM endpoints) is the optional self-update check — OFF by default.
- **API keys live in macOS Keychain** via `keyring`. They are never written to `~/.yogurt/config.toml`.
- **Audio is deleted as soon as transcription completes** (per-meeting toggle to keep audio coming in v1.1).
- **All persistent data is local**: `~/.yogurt/db.sqlite` + `~/.yogurt/notes/*.md`.

## License

MIT — see [LICENSE](LICENSE).
```

- [ ] **Step 3: Write `CONTRIBUTING.md`.**

```markdown
# Contributing to yogurt

Thanks for considering a contribution! This document covers the dev environment and the workspace layout.

## Dev environment

```bash
# Required
brew install rust pnpm

# Clone + install
git clone https://github.com/jarvisrchen/yogurt.git
cd yogurt
pnpm --dir web install

# Two-terminal dev loop
pnpm --dir web dev                           # frontend HMR on :5173
cargo run -p yogurt -- start --dev           # backend on :7878
```

The Rust backend at `:7878` proxies all non-API requests to the Vite dev server at `:5173`, so changes to the React app hot-reload while the backend stays running.

## Running tests

```bash
# Rust
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings

# Web
pnpm --dir web test
pnpm --dir web build
```

CI runs all of the above on every push and PR (`.github/workflows/ci.yml`).

### Snapshot tests

The markdown export uses [`insta`](https://insta.rs/) snapshots in `crates/yogurt-notes/tests/snapshots/`. When you intentionally change export output:

```bash
cargo insta review -p yogurt-notes
```

CI sets `INSTA_UPDATE=no`, so any drift fails the build. Always commit the updated `.snap` files alongside the code change.

## Workspace layout

```
yogurt/
├── crates/
│   ├── yogurt-cli/         # binary entrypoint — `yogurt start`, `yogurt doctor`
│   ├── yogurt-server/      # axum HTTP + WS, embeds web/dist via rust-embed
│   ├── yogurt-audio/       # ScreenCaptureKit + CoreAudio (mic + system loopback)
│   ├── yogurt-stt/         # STT trait + Deepgram, AssemblyAI, whisper.cpp adapters
│   ├── yogurt-llm/         # OpenAI-compatible client
│   ├── yogurt-db/          # SQLite via rusqlite + migrations
│   ├── yogurt-notes/       # markdown ↔ AST ↔ augmented-merge
│   └── yogurt-prompts/     # enhance.md + chat-system.md
└── web/                    # React + TipTap + Tailwind
```

Each crate is independently testable. Platform-specific code (`yogurt-audio`) sits behind a trait, so future Windows/Linux ports are additive.

## Code style

- **Rust:** `cargo fmt` + `cargo clippy -D warnings`. No exceptions in CI.
- **TypeScript:** Vite + `tsc --strict`. ESLint will land in a future polish pass; for now, just match surrounding style.
- **Commits:** [Conventional Commits](https://www.conventionalcommits.org/) — `feat:`, `fix:`, `chore:`, `docs:`, `test:`, `ci:`. Scoped where useful: `feat(cli): ...`, `fix(server): ...`.

## How to add a new STT or LLM provider

- **STT:** implement the `Stt` trait in `crates/yogurt-stt/src/`. Add a settings card in `web/src/routes/Settings.tsx`. Reference Deepgram or whisper.cpp for the shape.
- **LLM:** if the provider is OpenAI-compatible (most are), add a preset card to `web/src/routes/Settings.tsx` — no Rust changes needed. If it's a new protocol, extend `crates/yogurt-llm/`.

## Filing issues

Run `yogurt doctor --json` and paste the output. The diagnostic dump never includes API keys or note content — only versions and presence/absence flags.

## License

MIT. By contributing, you agree your contributions are licensed under MIT.
```

- [ ] **Step 4: Commit.**

```bash
git add README.md CONTRIBUTING.md docs/screenshots/
git commit -m "docs: polish README with badges + install matrix + privacy posture; add CONTRIBUTING.md"
```

---

### Task 9.13 · Release dry-run + cut v0.1.0

**Files:**
- Create: `scripts/release-checklist.md` (the appendix in this plan, mirrored as a file)

- [ ] **Step 1: Write `scripts/release-checklist.md`.**

(Mirror the "Release checklist" appendix below — same content. Having it as a file means future releases don't need to re-open this plan.)

```markdown
# Release checklist (v0.1.0 and beyond)

1. All open issues for the milestone are closed or moved.
2. `main` is green on CI.
3. `Cargo.toml` workspace version is the version you're about to release.
4. `CHANGELOG.md` has the entry, dated, with the right items.
5. Dry-run the release workflow:
   ```bash
   gh workflow run release.yml -f dry-run=true
   gh run watch
   ```
   Verify both tarballs build, SHA256SUMS is generated, no real release is created.
6. Tag and push (this triggers the real release):
   ```bash
   git tag -a vX.Y.Z -m "yogurt vX.Y.Z"
   git push origin vX.Y.Z
   gh run watch
   ```
7. Confirm the GitHub Release is live with both tarballs + SHA256SUMS attached.
8. Confirm the auto-opened PR on `jarvisrchen/homebrew-yogurt` is green — merge it.
9. Confirm crates.io shows the new version: `cargo search yogurt`.
10. Smoke-test all three install channels (fresh shell, no cached binaries):
    ```bash
    brew uninstall yogurt; brew untap jarvisrchen/yogurt
    brew tap jarvisrchen/yogurt && brew install yogurt && yogurt --version
    cargo install yogurt --force && yogurt --version
    curl -L https://github.com/jarvisrchen/yogurt/releases/download/vX.Y.Z/yogurt-aarch64-apple-darwin.tar.gz | tar xz
    ./yogurt --version
    ```
11. Update the README screenshot links if any visual changes shipped.
12. Tweet / post / announce. Note the macOS Gatekeeper bypass in the announcement.

**Publish hygiene rule:** never trigger a manual tap PR or cargo publish *before* the GitHub Release artifacts are uploaded. The workflow enforces this via `needs:` ordering — keep it that way.
```

- [ ] **Step 2: Run the dry-run workflow.**

```bash
git push origin main
gh workflow run release.yml -f dry-run=true
gh run watch
```

Expected: both matrix builds complete (~6-8 min each), tarballs are uploaded as artifacts, the `release` job runs in dry-run mode (prints "DRY RUN — would upload..."), the `tap` and `publish` jobs are skipped (`if: github.event_name == 'push'`).

If anything fails: read the logs, fix, push, re-run. **Do not** tag v0.1.0 until the dry-run is fully green.

- [ ] **Step 3: Verify dry-run artifacts.**

Download the artifact tarballs and smoke them locally:

```bash
gh run download <run-id>
tar -xzf yogurt-aarch64-apple-darwin.tar.gz
./yogurt --version       # should print yogurt 0.1.0
./yogurt doctor
./yogurt start --no-open &
sleep 1
curl localhost:7878/api/health
kill %1
```

Expected: binary runs, doctor outputs cleanly, server boots.

- [ ] **Step 4: Cut v0.1.0 — only with explicit user confirmation.**

This pushes a public tag and triggers the real release pipeline. Confirm with the user before running:

```bash
git tag -a v0.1.0 -m "yogurt v0.1.0 — first public release"
git push origin v0.1.0
gh run watch
```

- [ ] **Step 5: Validate all three install channels.**

Follow checklist item 10 above on a clean shell.

- [ ] **Step 6: Run the four PRD §14 acceptance criteria end-to-end.**

(See "Phase 9 acceptance criteria" below.)

- [ ] **Step 7: Commit the checklist file.**

```bash
git add scripts/release-checklist.md
git commit -m "docs: add release checklist for v0.1.0 and beyond"
```

---

## Phase 9 acceptance criteria

The four PRD §14 criteria all pass:

1. **New user e2e**: starting from a Mac with no yogurt installed, `brew install jarvisrchen/yogurt/yogurt && yogurt start` succeeds, the Screen Recording grant flow works, pasting a Minimax key in Settings unlocks the model section, and recording + transcribing + augmented-notes works for a real 5-minute test meeting.
2. **Visual parity**: side-by-side compare the black-user / grey-AI render against the design board screenshots in `yogurt-app-design/project/screenshots/`. Headings match, indentation matches, deep-link styling (`↳ HH:MM` dotted-underline lilac) matches. No regression vs Phase 4's hero work.
3. **Fully local**: switch transcription to `whisper.cpp · small.en` and the LLM to a local Ollama endpoint. Disable WiFi. Record a meeting, hit End — augmented notes appear with no network calls. (Verify with `lsof -i` or Little Snitch during the meeting.)
4. **Dev env in < 5 min from cold**: on a fresh Mac (or a fresh `rm -rf ~/.cargo ~/.rustup ~/Library/pnpm-store`), time the sequence: `brew install rust pnpm` + `git clone` + `pnpm --dir web install` + `cargo run -p yogurt -- start`. Should complete and show the React app under 5 minutes. If `cargo build --release` is slower, that's acceptable — the criterion is "working dev env", not full release build.

Additionally, all of the following must hold:

- `cargo test --workspace` passes (including new insta snapshots + recovery + doctor + crash tests).
- `pnpm --dir web test && pnpm --dir web build` passes.
- `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings` is clean.
- `cargo publish -p yogurt --dry-run` succeeds.
- The release workflow dry-run produces both tarballs + SHA256SUMS without errors.
- `yogurt doctor` outputs all expected fields on a fresh machine and is JSON-parseable with `--json`.
- After `kill -9` mid-meeting, the next `yogurt start` shows the recovery banner and the meeting markdown file is intact.

## What this phase does NOT do

- **No new product features.** Anything missing from PRD §5.1-5.11 is either a v1 bug (fix under the originating phase, not here) or a v2+ deferred item (PRD §6).
- **No notarization.** Documented bypass in README.
- **No Sentry / crash reporting service.** Crash markers stay local at `~/.yogurt/last_crash.json`.
- **No automatic update install.** The self-update check (off by default) only *reports* a new version; the user has to `brew upgrade yogurt` or `cargo install yogurt --force` themselves.
- **No universal `lipo` binary.** Per-arch tarballs only. The Homebrew formula branches on `Hardware::CPU.arm?`.
- **No PR opened against the tap before the GitHub Release exists.** Enforced by `needs:` ordering in the workflow.

## Appendix A — Release checklist for v0.1.0

(Mirrored at `scripts/release-checklist.md` for reuse on future releases.)

1. All open issues for the milestone are closed or moved to v1.1.
2. `main` is green on CI.
3. `Cargo.toml` workspace version is `0.1.0`.
4. `CHANGELOG.md` v0.1.0 entry is dated and complete.
5. Dry-run the release workflow: `gh workflow run release.yml -f dry-run=true && gh run watch`. Verify tarballs build cleanly.
6. Tag: `git tag -a v0.1.0 -m "yogurt v0.1.0 — first public release" && git push origin v0.1.0`.
7. Watch the release run: `gh run watch`. Three jobs (`build` matrix, `release`, `tap`, `publish`) must all go green.
8. Verify the GitHub Release is live at https://github.com/jarvisrchen/yogurt/releases/tag/v0.1.0 with both tarballs + SHA256SUMS.
9. Merge the auto-opened PR on `jarvisrchen/homebrew-yogurt`.
10. Verify crates.io: `cargo search yogurt` shows `yogurt = "0.1.0"`.
11. Smoke-test all three install channels in a fresh shell (see Task 9.13 step 5).
12. Announce. Include the macOS Gatekeeper bypass note.

## Appendix B — Publish hygiene rule

**Never push a Homebrew formula update or run `cargo publish` before the GitHub Release artifacts are live.**

The race condition: if the formula points at `https://github.com/jarvisrchen/yogurt/releases/download/v0.1.0/yogurt-aarch64-apple-darwin.tar.gz` and that URL 404s, every `brew install` for the window before the release is published fails — and the failures are cached by Homebrew's mirror infrastructure, so the bad state outlives the fix.

The release workflow enforces this with `needs:` ordering:

```
build → release → tap   (tap waits for release, cannot run on a failed release)
                ↘
                  publish   (publish waits for both release AND tap)
```

If you ever manually run any of these steps out of order (don't), the hygiene rule is: **GitHub Release artifacts are live first**, then formula PR, then crates.io. Reverse this order and you generate cached failures across user machines.

## Next plan

After Phase 9 lands and v0.1.0 ships, the next plan is the v1.1 milestone — likely `docs/superpowers/plans/<date>-yogurt-v1.1-notarization-and-keep-audio.md` covering:

- Apple Developer ID enrollment + notarization on the release workflow (removes the Gatekeeper warning).
- Per-meeting "keep audio" toggle (PRD §4 Q7 footnote — trivial schema addition).
- Polish items pulled from real-user issues filed against v0.1.0.

Beyond v1.1, the v2 roadmap in PRD §6 takes over (calendar integration, template picker + versions rail, cross-meeting chat, etc.).
