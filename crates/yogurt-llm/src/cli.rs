//! Local agent-CLI adapter (`docs/TODO.md` LLM-4).
//!
//! Spawns a locally-installed coding-agent CLI (`claude -p` or
//! `cursor-agent -p`) as a one-shot text-completion backend, for a user who
//! wants to run an agent CLI they already have installed as their LLM
//! provider instead of any cloud provider at all. `yogurt-server` reaches
//! this only when the active `providers` row is explicitly `adapter:
//! "cli"` - see AGENTS.md's amended "one process" constraint. There is
//! deliberately no automatic fallback from an unreachable HTTP provider to
//! this: an earlier draft (LLM-1) did that and was reverted, because
//! silently rerouting a meeting's real content to a different backend on a
//! network hiccup is a behavior change a user should opt into, not one
//! that happens to them.
//!
//! Both CLIs are treated as opaque `-p --output-format json` binaries that
//! print one JSON object with a `result` string and an `is_error` bool.
//! Verified against a live `claude` binary; `cursor-agent` support is
//! inferred from its documented Claude-Code-compatible interface and not
//! verified against a live binary here - a wrong assumption fails as a
//! clear "invalid CLI output" error, not silent corruption.

use crate::{ChatChunk, ChatMessage, ChatRequest, ChatResponse, LlmClient};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use futures_util::stream::{self, BoxStream, StreamExt};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;

/// Same ceiling as `OpenAiCompatClient`'s HTTP timeout - a wedged CLI
/// process (auth prompt with no TTY to answer it, a huge transcript) must
/// not hang the enhance/chat handler indefinitely.
const CLI_TIMEOUT: Duration = crate::HTTP_TIMEOUT;

/// Which agent CLI backs a [`CliClient`]. Public so `yogurt-db`'s
/// `providers` table (LLM-4: explicit CLI provider selection) and
/// `yogurt-server` can name a program without either crate hardcoding its
/// binary name string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliProgram {
    Claude,
    CursorAgent,
}

impl CliProgram {
    pub fn binary_name(self) -> &'static str {
        match self {
            CliProgram::Claude => "claude",
            CliProgram::CursorAgent => "cursor-agent",
        }
    }

    /// Parse the id a `providers.model` row stores for a `cli`-adapter
    /// provider (see `yogurt_db::providers::adapter::CLI`) back into a
    /// `CliProgram`. Currently identical to `binary_name`, kept as a
    /// separate round-trip pair rather than assuming that stays true.
    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "claude" => Some(CliProgram::Claude),
            "cursor-agent" => Some(CliProgram::CursorAgent),
            _ => None,
        }
    }
}

/// Local agent-CLI adapter. Constructed only via [`CliClient::locate`],
/// which resolves the binary path once so a request never has to
/// distinguish "not installed" from "installed but erroring".
pub struct CliClient {
    binary: PathBuf,
    program: CliProgram,
    /// `--model` value, e.g. `"sonnet"` / `"opus"` / `"haiku"` for
    /// `claude`. `None` uses the CLI's own default model (whatever a bare
    /// `claude -p ...` would pick). The Settings UI exposes this as a free-
    /// text field with static suggestions per program - enhance/chat are
    /// simple extraction calls, not reasoning-heavy, so the cheaper tiers
    /// are usually the right choice, not whatever the CLI defaults to.
    model: Option<String>,
}

impl CliClient {
    /// Resolve `program` on `$PATH`, or a clear `Err` naming it if it
    /// isn't there. Never silently substitutes the other CLI - the caller
    /// (LLM-4: the active `cli`-adapter provider row) asked for this one
    /// specifically. `model` is the provider row's `cli_model` column,
    /// verbatim - not validated here, since neither CLI exposes a static
    /// list to validate against (`cursor-agent --list-models` requires its
    /// own process spawn); an invalid value just surfaces as a normal CLI
    /// error on the next call.
    pub fn locate(program: CliProgram, model: Option<String>) -> Result<Self> {
        find_on_path(program.binary_name())
            .map(|binary| Self {
                binary,
                program,
                model,
            })
            .ok_or_else(|| anyhow!("{} not found on $PATH", program.binary_name()))
    }

    pub fn program(&self) -> CliProgram {
        self.program
    }

    /// Build the argv (excluding the binary itself) for one completion
    /// call. Split out from `run` so the containment flags are unit-
    /// testable without spawning a process.
    fn build_args(&self, prompt: &str) -> Vec<String> {
        let mut args = vec![
            "-p".to_string(),
            prompt.to_string(),
            "--output-format".to_string(),
            "json".to_string(),
        ];
        match self.program {
            CliProgram::Claude => {
                // Meeting-transcript content is untrusted - whatever the
                // other party on the call said - and it reaches this CLI
                // as plain prompt text, not through a reviewed tool-use
                // loop. Lock it down so a transcript that looks like an
                // instruction can't do anything but produce text: no
                // shell/code tools or WebFetch, no MCP servers, no skills,
                // and this repo's own `.claude/` settings are ignored so a
                // permissive local Bash rule can't leak into a subprocess
                // whose input we don't trust.
                args.push("--restricted".to_string());
                args.push("--strict-mcp-config".to_string());
                args.push("--disable-slash-commands".to_string());
                // Enhance/chat are extraction calls, not reasoning tasks -
                // low effort is faster and cheaper with no real quality
                // loss for "merge these notes with this transcript"-shaped
                // prompts. Unconditional, not user-configurable (unlike
                // `--model`): there's no scenario here that benefits from
                // spending more.
                args.push("--effort".to_string());
                args.push("low".to_string());
            }
            CliProgram::CursorAgent => {
                // `--trust` clears the one-time "trust this workspace?"
                // prompt every call would otherwise hit (each call gets a
                // fresh, never-seen-before scratch cwd, so there is no
                // "already trusted" state to carry over). `--sandbox
                // enabled` is the closest documented equivalent to
                // claude's `--restricted` - cursor-agent's docs don't list
                // a per-tool restriction flag, so this is the containment
                // available for the same untrusted-transcript-input
                // reasoning as claude above. Deliberately NOT `--force` /
                // `--yolo` ("force allow commands unless explicitly
                // denied") - that is the broad auto-approval flag, the
                // cursor-agent analogue of `--dangerously-skip-
                // permissions`, and granting it here would remove exactly
                // the protection `--sandbox enabled` provides. Unverified
                // against a live binary (see module doc) - if `--sandbox`
                // turns out not to exist on some version, the whole call
                // fails loudly rather than silently running unsandboxed.
                args.push("--trust".to_string());
                args.push("--sandbox".to_string());
                args.push("enabled".to_string());
            }
        }
        if let Some(model) = &self.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }
        args
    }

    async fn run(&self, req: &ChatRequest) -> Result<String> {
        let prompt = flatten(&req.messages);

        // `--restricted` confines the file tools to the working directory
        // rather than removing them, so give it an empty scratch dir with
        // nothing sensitive to read and nowhere useful to write.
        let scratch = std::env::temp_dir().join(format!("yogurt-cli-llm-{}", uuid_v4_ish()));
        std::fs::create_dir_all(&scratch)
            .with_context(|| format!("failed to create scratch dir {}", scratch.display()))?;

        let mut cmd = tokio::process::Command::new(&self.binary);
        cmd.kill_on_drop(true)
            .current_dir(&scratch)
            .args(self.build_args(&prompt))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let result = tokio::time::timeout(CLI_TIMEOUT, cmd.output()).await;
        let _ = std::fs::remove_dir_all(&scratch);

        let output = result
            .map_err(|_| {
                anyhow!(
                    "{} CLI timed out after {}s",
                    self.program.binary_name(),
                    CLI_TIMEOUT.as_secs()
                )
            })?
            .with_context(|| format!("failed to spawn {} CLI", self.program.binary_name()))?;

        interpret_output(
            self.program.binary_name(),
            &output.stdout,
            &output.stderr,
            output.status,
        )
    }
}

#[async_trait]
impl LlmClient for CliClient {
    /// `"cli:claude"`, or `"cli:claude:haiku"` when a `--model` override is
    /// set. The suffix matters here specifically because `test_provider`'s
    /// cli branch echoes this straight back as the Settings `Test`
    /// button's verdict ("answered as …") - without it, testing a
    /// `haiku`-pinned row and a default-model row look identical, and the
    /// user has no way to tell the override was actually exercised.
    fn model_name(&self) -> String {
        match &self.model {
            Some(model) => format!("cli:{}:{model}", self.program.binary_name()),
            None => format!("cli:{}", self.program.binary_name()),
        }
    }

    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse> {
        let content = self.run(&req).await?;
        let model = self.model_name();
        Ok(ChatResponse { content, model })
    }

    async fn stream(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ChatChunk>>> {
        // v1 scope (LLM-4): no `--output-format stream-json` parsing yet.
        // Buffer the one-shot result into a single delta, same pattern as
        // `MockLlm::stream` in yogurt-server.
        let full = self.complete(req).await?.content;
        let s = stream::iter(vec![
            Ok(ChatChunk {
                delta: full,
                done: false,
            }),
            Ok(ChatChunk {
                delta: String::new(),
                done: true,
            }),
        ]);
        Ok(s.boxed())
    }
}

#[derive(Deserialize)]
struct CliResult {
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    result: String,
}

/// Turn a finished process's stdout/stderr/exit status into either the
/// completion text or a clear error. Pure and synchronous so the exact bug
/// this was written to fix - checking `status.success()` before ever
/// looking at stdout - has a deterministic regression test that doesn't
/// need a real subprocess.
///
/// `claude --output-format json` still writes a structured result to
/// stdout on many failures (confirmed live: "not logged in" exits 1 with
/// `{"is_error":true,"result":"Not logged in · Please run /login"}` on
/// stdout, empty stderr) - the whole point of asking for JSON output is a
/// machine-readable error, not just a success payload. So stdout is parsed
/// FIRST regardless of exit status, and the raw exit-status/stderr message
/// is only the fallback for when stdout isn't parseable JSON at all (the
/// CLI crashed before ever reaching its own output formatting - a bad
/// flag, a missing binary dependency, etc.).
fn interpret_output(
    program_name: &str,
    stdout: &[u8],
    stderr: &[u8],
    status: std::process::ExitStatus,
) -> Result<String> {
    let stdout = String::from_utf8_lossy(stdout);
    let Ok(parsed) = serde_json::from_str::<CliResult>(stdout.trim()) else {
        let stderr = String::from_utf8_lossy(stderr);
        bail!("{program_name} CLI exited with {status}: {}", stderr.trim());
    };
    if parsed.is_error {
        bail!("{program_name} CLI reported an error: {}", parsed.result);
    }
    Ok(parsed.result)
}

/// Flatten a chat history into the single positional prompt argument both
/// CLIs take. Each call is a fresh process with no session to resume, so
/// this mirrors what `OpenAiCompatClient` does anyway: send the full
/// history every time, not just the latest turn.
fn flatten(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .map(|m| format!("[{}]\n{}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `$PATH` lookup for an executable file named `name`. Std-only rather
/// than a `which` dependency - it's one loop over `PATH`, called once per
/// LLM resolution, not a hot path.
fn find_on_path(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(name);
        let meta = std::fs::metadata(&candidate).ok()?;
        (meta.is_file() && meta.permissions().mode() & 0o111 != 0).then_some(candidate)
    })
}

/// Good-enough unique suffix for a scratch dir name - collision would only
/// matter if two calls raced within the same process's `temp_dir()`, and
/// `std::process::id()` + a monotonic counter rules that out without
/// pulling in a UUID crate for one throwaway directory name.
fn uuid_v4_ish() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_on_path_locates_a_known_binary() {
        // /bin/sh exists and is executable on every macOS install (hard
        // constraint: macOS 13+ only), so this is portable enough without
        // hardcoding a path.
        let found = find_on_path("sh").expect("sh should be on PATH");
        assert!(found.ends_with("sh"));
    }

    #[test]
    fn find_on_path_returns_none_for_missing_binary() {
        assert!(find_on_path("definitely-not-a-real-binary-xyz").is_none());
    }

    #[test]
    fn flatten_labels_each_message_by_role() {
        let messages = vec![ChatMessage::system("be terse"), ChatMessage::user("hi")];
        let flat = flatten(&messages);
        assert_eq!(flat, "[system]\nbe terse\n\n[user]\nhi");
    }

    #[test]
    fn build_args_restricts_claude_and_sandboxes_cursor_agent() {
        let claude = CliClient {
            binary: PathBuf::from("claude"),
            program: CliProgram::Claude,
            model: None,
        };
        let args = claude.build_args("prompt");
        assert!(args.contains(&"--restricted".to_string()));
        assert!(args.contains(&"--strict-mcp-config".to_string()));
        assert!(args.contains(&"--disable-slash-commands".to_string()));
        assert!(!args.contains(&"--model".to_string()));

        let cursor = CliClient {
            binary: PathBuf::from("cursor-agent"),
            program: CliProgram::CursorAgent,
            model: None,
        };
        let args = cursor.build_args("prompt");
        assert!(!args.contains(&"--restricted".to_string()));
        assert!(!args.contains(&"--force".to_string()));
        assert!(!args.contains(&"--yolo".to_string()));
        assert_eq!(
            args,
            vec![
                "-p",
                "prompt",
                "--output-format",
                "json",
                "--trust",
                "--sandbox",
                "enabled"
            ]
        );
    }

    #[test]
    fn build_args_appends_model_when_set() {
        let claude = CliClient {
            binary: PathBuf::from("claude"),
            program: CliProgram::Claude,
            model: Some("haiku".to_string()),
        };
        let args = claude.build_args("prompt");
        let model_idx = args.iter().position(|a| a == "--model").unwrap();
        assert_eq!(args[model_idx + 1], "haiku");
    }

    #[test]
    fn cli_result_parses_success_and_error_shapes() {
        let ok: CliResult = serde_json::from_str(r#"{"is_error":false,"result":"hi"}"#).unwrap();
        assert!(!ok.is_error);
        assert_eq!(ok.result, "hi");

        let err: CliResult =
            serde_json::from_str(r#"{"is_error":true,"result":"not logged in"}"#).unwrap();
        assert!(err.is_error);
        assert_eq!(err.result, "not logged in");
    }

    #[test]
    fn model_name_is_prefixed_by_cli() {
        let claude = CliClient {
            binary: PathBuf::from("claude"),
            program: CliProgram::Claude,
            model: None,
        };
        assert_eq!(claude.model_name(), "cli:claude");
    }

    #[test]
    fn model_name_includes_the_model_override_when_set() {
        let claude = CliClient {
            binary: PathBuf::from("claude"),
            program: CliProgram::Claude,
            model: Some("haiku".to_string()),
        };
        assert_eq!(claude.model_name(), "cli:claude:haiku");
    }

    // ── interpret_output ────────────────────────────────────────────────
    // Regression coverage for a bug caught by manually running the real
    // `claude` CLI against a "not logged in" account: it exits 1 but still
    // writes a structured, actionable error to stdout as valid JSON. The
    // first cut of this function checked `status.success()` before ever
    // looking at stdout, so the UI showed a useless "exited with status 1:
    // " instead of "Not logged in - please run /login".

    use std::os::unix::process::ExitStatusExt;

    fn exit_status(code: i32) -> std::process::ExitStatus {
        std::process::ExitStatus::from_raw(code)
    }

    #[test]
    fn interpret_output_surfaces_the_json_error_even_on_nonzero_exit() {
        let stdout = r#"{"is_error":true,"result":"Not logged in · Please run /login"}"#;
        let err = match interpret_output("claude", stdout.as_bytes(), b"", exit_status(1)) {
            Err(e) => e,
            Ok(_) => panic!("expected an error"),
        };
        assert_eq!(
            err.to_string(),
            "claude CLI reported an error: Not logged in · Please run /login"
        );
    }

    #[test]
    fn interpret_output_returns_the_result_on_success() {
        let stdout = br#"{"is_error":false,"result":"PONG"}"#;
        let got = interpret_output("claude", stdout, b"", exit_status(0)).unwrap();
        assert_eq!(got, "PONG");
    }

    #[test]
    fn interpret_output_falls_back_to_exit_status_when_stdout_is_not_json() {
        // A crash before the CLI's own output formatting ever ran - a bad
        // flag, a missing shared library, etc. - has no JSON to parse, so
        // this is the one case that should still use stderr + exit status.
        let err = match interpret_output(
            "claude",
            b"",
            b"error: unknown option '--not-a-real-flag'\n",
            exit_status(1),
        ) {
            Err(e) => e,
            Ok(_) => panic!("expected an error"),
        };
        assert!(err.to_string().contains("unknown option"));
        assert!(err.to_string().contains("claude CLI exited with"));
    }

    #[test]
    fn cli_program_parse_round_trips_binary_name() {
        assert_eq!(CliProgram::parse("claude"), Some(CliProgram::Claude));
        assert_eq!(
            CliProgram::parse("cursor-agent"),
            Some(CliProgram::CursorAgent)
        );
        assert_eq!(CliProgram::parse("gpt-4o"), None);
        assert_eq!(CliProgram::parse(""), None);
    }

    #[test]
    fn locate_finds_a_program_actually_on_path() {
        // Reuse the same "sh is always present on macOS" trick as
        // `find_on_path_locates_a_known_binary` by locating the real
        // `binary_name` and just asserting `locate` returns a matching
        // path when it exists, without asserting on the not-found branch -
        // whether `claude`/`cursor-agent` are installed is environment-
        // dependent, so that branch is exercised in `CliClient::run`'s
        // error path instead (`{program} not found on $PATH`), not here.
        if let Some(binary) = find_on_path(CliProgram::Claude.binary_name()) {
            let client = CliClient::locate(CliProgram::Claude, None).expect("claude is on PATH");
            assert_eq!(client.binary, binary);
            assert_eq!(client.program(), CliProgram::Claude);
        }
    }

    #[test]
    fn locate_names_the_program_when_not_found() {
        // `binary_name` can't be swapped out (only two real variants
        // exist), so this only proves something when the machine running
        // the test doesn't have cursor-agent installed - true in CI and on
        // most dev machines, but not asserted as a hard requirement.
        if find_on_path(CliProgram::CursorAgent.binary_name()).is_none() {
            // `CliClient` isn't `Debug`, so match explicitly rather than
            // `Result::unwrap_err` (which requires `T: Debug`).
            match CliClient::locate(CliProgram::CursorAgent, None) {
                Err(e) => assert!(e.to_string().contains("cursor-agent")),
                Ok(_) => panic!("expected an error when cursor-agent isn't on PATH"),
            }
        }
    }
}
