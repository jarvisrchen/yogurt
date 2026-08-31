//! Local agent-CLI fallback adapter (`docs/TODO.md` LLM-1).
//!
//! Spawns a locally-installed coding-agent CLI (`claude -p` or
//! `cursor-agent -p`) as a one-shot text-completion backend, for the one
//! case an HTTP provider can't cover: corporate egress that allows the
//! agent CLI's own traffic but blocks the configured LLM provider's
//! `base_url`. `yogurt-server::llm_openai::resolve` wires this in as a
//! fallback behind the configured provider, never as a provider row in
//! its own right - see AGENTS.md's amended "one process" constraint.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Program {
    Claude,
    CursorAgent,
}

impl Program {
    fn binary_name(self) -> &'static str {
        match self {
            Program::Claude => "claude",
            Program::CursorAgent => "cursor-agent",
        }
    }
}

/// Local agent-CLI adapter. Constructed only via [`CliClient::discover`],
/// which resolves the binary path once so a request never has to
/// distinguish "not installed" from "installed but erroring".
pub struct CliClient {
    binary: PathBuf,
    program: Program,
}

impl CliClient {
    /// Search `$PATH` for `claude`, then `cursor-agent`; return the first
    /// one found. Order is a preference, not a correctness requirement -
    /// only one fallback client is ever constructed per resolution, and
    /// either beats `MockLlm` or a hard failure when the configured
    /// provider is unreachable.
    pub fn discover() -> Option<Self> {
        [Program::Claude, Program::CursorAgent]
            .into_iter()
            .find_map(|program| {
                find_on_path(program.binary_name()).map(|binary| Self { binary, program })
            })
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
        if self.program == Program::Claude {
            // Meeting-transcript content is untrusted - whatever the other
            // party on the call said - and it reaches this CLI as plain
            // prompt text, not through a reviewed tool-use loop. Lock it
            // down so a transcript that looks like an instruction can't do
            // anything but produce text: no shell/code tools or WebFetch,
            // no MCP servers, no skills, and this repo's own `.claude/`
            // settings are ignored so a permissive local Bash rule can't
            // leak into a subprocess whose input we don't trust.
            args.push("--restricted".to_string());
            args.push("--strict-mcp-config".to_string());
            args.push("--disable-slash-commands".to_string());
        }
        // Not applied to `cursor-agent`: these three flags are unverified
        // against a live binary (see module doc), and passing a flag that
        // doesn't exist fails the whole call rather than degrading safely.
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

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "{} CLI exited with {}: {}",
                self.program.binary_name(),
                output.status,
                stderr.trim()
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: CliResult = serde_json::from_str(stdout.trim()).with_context(|| {
            format!(
                "invalid {} CLI JSON output: {}",
                self.program.binary_name(),
                stdout.trim()
            )
        })?;
        if parsed.is_error {
            bail!(
                "{} CLI reported an error: {}",
                self.program.binary_name(),
                parsed.result
            );
        }
        Ok(parsed.result)
    }
}

#[async_trait]
impl LlmClient for CliClient {
    fn model_name(&self) -> String {
        format!("cli:{}", self.program.binary_name())
    }

    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse> {
        let content = self.run(&req).await?;
        let model = self.model_name();
        Ok(ChatResponse { content, model })
    }

    async fn stream(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ChatChunk>>> {
        // v1 scope (LLM-1): no `--output-format stream-json` parsing yet.
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
    fn build_args_restricts_claude_but_not_cursor_agent() {
        let claude = CliClient {
            binary: PathBuf::from("claude"),
            program: Program::Claude,
        };
        let args = claude.build_args("prompt");
        assert!(args.contains(&"--restricted".to_string()));
        assert!(args.contains(&"--strict-mcp-config".to_string()));
        assert!(args.contains(&"--disable-slash-commands".to_string()));

        let cursor = CliClient {
            binary: PathBuf::from("cursor-agent"),
            program: Program::CursorAgent,
        };
        let args = cursor.build_args("prompt");
        assert!(!args.contains(&"--restricted".to_string()));
        assert_eq!(args, vec!["-p", "prompt", "--output-format", "json"]);
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
            program: Program::Claude,
        };
        assert_eq!(claude.model_name(), "cli:claude");
    }
}
