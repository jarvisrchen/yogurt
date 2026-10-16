//! Local agent-CLI adapter: runs `claude -p`, `cursor-agent -p` or `opencode run` as a one-shot completion backend.
//!
//! `yogurt-server` reaches this only when the active `providers` row is explicitly `adapter: "cli"`.
//! There is deliberately no automatic fallback from an unreachable HTTP provider to this,
//! because silently rerouting a meeting's content to a different backend is a change the user must opt into.
//!
//! `claude` and `cursor-agent` print one JSON object with `result` and `is_error`.
//! `opencode run --format json` prints JSON lines instead, so [`interpret_output`] dispatches on the program.

use crate::{ChatChunk, ChatMessage, ChatRequest, ChatResponse, LlmClient};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use futures_util::stream::{self, BoxStream, StreamExt};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;

/// Hard ceiling on one CLI subprocess, so a wedged process (auth prompt with no TTY, a huge transcript) cannot hang the handler.
/// Sized off `GENERATION_TIMEOUT` because it bounds a whole generation.
/// `response_timeout` is what the caller waits; this is the inner guard.
const CLI_TIMEOUT: Duration = crate::GENERATION_TIMEOUT;

/// Hard ceiling on a [`list_models`] probe: one catalog fetch with a user watching the Refresh button.
const LIST_MODELS_TIMEOUT: Duration = Duration::from_secs(30);

/// Which agent CLI backs a [`CliClient`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliProgram {
    Claude,
    CursorAgent,
    OpenCode,
}

impl CliProgram {
    /// Every variant, in preset order. The single place the set is enumerated.
    pub const ALL: &'static [CliProgram] = &[
        CliProgram::Claude,
        CliProgram::CursorAgent,
        CliProgram::OpenCode,
    ];

    pub fn binary_name(self) -> &'static str {
        match self {
            CliProgram::Claude => "claude",
            CliProgram::CursorAgent => "cursor-agent",
            CliProgram::OpenCode => "opencode",
        }
    }

    /// Parse the id a `providers.model` row stores for a `cli`-adapter provider.
    pub fn parse(id: &str) -> Option<Self> {
        CliProgram::ALL
            .iter()
            .copied()
            .find(|p| p.binary_name() == id)
    }

    /// The argument that makes this CLI print its `--model` catalog and exit, or `None` if it has no such mode.
    ///
    /// `claude` has none: its four aliases ship as static preset suggestions.
    /// The other two have hundreds of ids that vary by account and plan.
    pub fn list_models_arg(self) -> Option<&'static str> {
        match self {
            CliProgram::Claude => None,
            CliProgram::CursorAgent => Some("--list-models"),
            CliProgram::OpenCode => Some("models"),
        }
    }
}

/// Ask `program` for the model ids its `--model` flag accepts.
///
/// A successful listing does not mean the models are usable.
/// The catalog is not plan-filtered: a free-tier cursor account lists 200+ ids and rejects every named one at call time.
/// `opencode models` lists models of providers with no credit left.
/// `test_provider`'s real completion is the only entitlement signal.
pub async fn list_models(program: CliProgram) -> Result<Vec<String>> {
    let flag = program
        .list_models_arg()
        .ok_or_else(|| anyhow!("{} has no model-list command", program.binary_name()))?;
    let binary = find_on_path(program.binary_name())
        .ok_or_else(|| anyhow!("{} not found on $PATH", program.binary_name()))?;

    let mut cmd = tokio::process::Command::new(&binary);
    cmd.kill_on_drop(true)
        .arg(flag)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = tokio::time::timeout(LIST_MODELS_TIMEOUT, cmd.output())
        .await
        .map_err(|_| {
            anyhow!(
                "{} {flag} timed out after {}s",
                program.binary_name(),
                LIST_MODELS_TIMEOUT.as_secs()
            )
        })?
        .with_context(|| format!("failed to spawn {} CLI", program.binary_name()))?;

    if !output.status.success() {
        // This flag has no structured output to parse an error from, so stderr is all there is (a not-logged-in CLI lands here).
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        bail!(
            "{} {flag} failed{}",
            program.binary_name(),
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        );
    }

    let models = parse_model_list(&String::from_utf8_lossy(&output.stdout));
    if models.is_empty() {
        bail!("{} {flag} returned no models", program.binary_name());
    }
    Ok(models)
}

/// Pull model ids out of a catalog listing's stdout. Neither CLI offers JSON for it, and they print different lines:
///
/// - `cursor-agent --list-models`: an `Available models` header, a blank line, then `<id> - <human label>` per line.
/// - `opencode models`: a bare `<provider>/<model>` per line.
///
/// A ` - ` separator marks the end of the id when present.
/// The whitespace filter drops cursor's header and any prose footer without knowing which format produced the line.
fn parse_model_list(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(|line| match line.split_once(" - ") {
            Some((id, _label)) => id.trim(),
            None => line.trim(),
        })
        .filter(|id| !id.is_empty() && !id.contains(char::is_whitespace))
        .map(str::to_string)
        .collect()
}

/// Constructed only via [`CliClient::locate`], which resolves the binary once so a request never has to tell "not installed" from "installed but erroring".
pub struct CliClient {
    binary: PathBuf,
    program: CliProgram,
    /// `--model` value. `None` uses the CLI's own default model.
    model: Option<String>,
}

impl CliClient {
    /// Resolve `program` on `$PATH`, or an `Err` naming it. Never substitutes the other CLI.
    ///
    /// `model` is passed through unvalidated: catalog membership does not imply the account may use the model (see [`list_models`]),
    /// so an invalid value surfaces as a normal CLI error on the next call.
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

    /// Build the argv (excluding the binary) for one completion call, split out so the containment flags are unit-testable.
    ///
    /// Fully per-program because the three CLIs share no argv shape.
    fn build_args(&self, prompt: &str) -> Vec<String> {
        let mut args: Vec<String> = match self.program {
            CliProgram::Claude => vec![
                "-p".to_string(),
                prompt.to_string(),
                "--output-format".to_string(),
                "json".to_string(),
                // Transcript content is untrusted and reaches the CLI as plain prompt text.
                // Lock it down to text output: no shell/code tools or WebFetch, no MCP servers, no skills,
                // and ignore this repo's `.claude/` settings so a permissive local Bash rule cannot leak in.
                "--restricted".to_string(),
                "--strict-mcp-config".to_string(),
                "--disable-slash-commands".to_string(),
                // Enhance and chat are extraction calls, so low effort is faster and cheaper with no real quality loss.
                //
                // Advisory, not a cap: haiku ignores it and spends ~10k thinking tokens (94 s) on a short answer, sonnet finishes in 4 s.
                // The CLI has no thinking-token budget to cap it, so callers must tolerate a model that ignores this.
                // That is why `response_timeout` is sized for generation.
                "--effort".to_string(),
                "low".to_string(),
            ],
            CliProgram::CursorAgent => vec![
                "-p".to_string(),
                prompt.to_string(),
                "--output-format".to_string(),
                "json".to_string(),
                // `--trust` clears the "trust this workspace?" prompt, which every call would hit because each gets a fresh scratch cwd.
                // `--sandbox enabled` is the closest equivalent to claude's `--restricted` for the same untrusted-input reason.
                // Never `--force`/`--yolo`: that broad auto-approval would remove the protection the sandbox provides.
                "--trust".to_string(),
                "--sandbox".to_string(),
                "enabled".to_string(),
            ],
            CliProgram::OpenCode => vec![
                "run".to_string(),
                "--format".to_string(),
                "json".to_string(),
                // Drops the user's third-party opencode plugins, which no permission entry covers.
                // Pairs with `OPENCODE_PERMISSION` in [`Self::env`].
                //
                // Never `--auto` (opencode's `--dangerously-skip-permissions`): the input is untrusted.
                "--pure".to_string(),
            ],
        };
        if let Some(model) = &self.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }
        // opencode takes the prompt as a trailing positional, after every flag.
        // It cannot look like a flag because `flatten` always prefixes a `[role]` line.
        if self.program == CliProgram::OpenCode {
            args.push(prompt.to_string());
        }
        args
    }

    /// Environment overrides for one completion call, on top of the inherited environment.
    ///
    /// Only `opencode` needs any, because its containment is not expressible in argv.
    /// A bare `opencode run` loads the user's global config, which offered the model Gmail send/search and Drive writes,
    /// an exfiltration path for a transcript that says "email yourself the following".
    /// `OPENCODE_CONFIG_CONTENT={}` does not close it.
    ///
    /// `OPENCODE_PERMISSION` `{"*":"deny"}` removes every tool from the model's schema.
    /// Enhance and chat never need a tool, and the request drops from 35k to 2.2k input tokens.
    fn env(&self) -> &'static [(&'static str, &'static str)] {
        match self.program {
            CliProgram::OpenCode => &[("OPENCODE_PERMISSION", r#"{"*":"deny"}"#)],
            CliProgram::Claude | CliProgram::CursorAgent => &[],
        }
    }

    async fn run(&self, req: &ChatRequest) -> Result<String> {
        let prompt = flatten(&req.messages);

        // `--restricted` confines file tools to the cwd, so give it an empty scratch dir with nothing sensitive to read.
        let scratch = std::env::temp_dir().join(format!("yogurt-cli-llm-{}", uuid_v4_ish()));
        std::fs::create_dir_all(&scratch)
            .with_context(|| format!("failed to create scratch dir {}", scratch.display()))?;

        let mut cmd = tokio::process::Command::new(&self.binary);
        cmd.kill_on_drop(true)
            .current_dir(&scratch)
            .args(self.build_args(&prompt))
            .envs(self.env().iter().copied())
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

        interpret_output(self.program, &output.stdout, &output.stderr, output.status)
    }
}

#[async_trait]
impl LlmClient for CliClient {
    /// `"cli:claude"`, or `"cli:claude:haiku"` when a `--model` override is set.
    /// `test_provider` echoes this as the Settings Test verdict, so the suffix shows the override was exercised.
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

    fn response_timeout(&self) -> Duration {
        // `stream` finishes the whole generation before yielding.
        crate::GENERATION_TIMEOUT
    }

    async fn stream(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ChatChunk>>> {
        // No stream-json parsing: buffer the one-shot result into a single delta.
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

/// One line of `opencode run --format json` output.
/// Only `"text"` (answer chunk in `part`) and `"error"` (in `error`) matter.
/// Other event types are ignored since opencode may add more, and `reasoning` must not be concatenated into the result.
#[derive(Deserialize)]
struct OpenCodeEvent {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    part: Option<OpenCodePart>,
    #[serde(default)]
    error: Option<OpenCodeError>,
}

#[derive(Deserialize)]
struct OpenCodePart {
    #[serde(default)]
    id: String,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct OpenCodeError {
    #[serde(default)]
    name: String,
    #[serde(default)]
    data: OpenCodeErrorData,
}

#[derive(Deserialize, Default)]
struct OpenCodeErrorData {
    #[serde(default)]
    message: String,
}

/// Turn a finished process's stdout, stderr and exit status into the completion text or an error.
/// Pure so the stdout-before-exit-status rule is testable without a subprocess.
fn interpret_output(
    program: CliProgram,
    stdout: &[u8],
    stderr: &[u8],
    status: std::process::ExitStatus,
) -> Result<String> {
    match program {
        CliProgram::OpenCode => interpret_jsonl(stdout, stderr, status),
        CliProgram::Claude | CliProgram::CursorAgent => {
            interpret_single_json(program.binary_name(), stdout, stderr, status)
        }
    }
}

/// `claude` / `cursor-agent`: one JSON object on stdout.
///
/// `--output-format json` still writes a structured result to stdout on many failures
/// ("not logged in" exits 1 with `{"is_error":true,...}` on stdout and empty stderr).
/// So stdout is parsed first regardless of exit status, and exit status plus stderr are the fallback
/// for when stdout is not JSON at all (a crash before the CLI's own output formatting).
fn interpret_single_json(
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
    // An empty `result` is a failed call, not an answer.
    if parsed.result.trim().is_empty() {
        bail!("{program_name} CLI returned no text output");
    }
    Ok(parsed.result)
}

/// `opencode run --format json`: JSON lines, one event per line.
///
/// Same stdout-before-exit-status rule as [`interpret_single_json`]: a failed run exits 1 having printed a structured `error` line.
///
/// Text parts are keyed by `part.id` and joined in first-appearance order,
/// so a part streamed as growing snapshots does not repeat the answer's prefix once per event.
fn interpret_jsonl(
    stdout: &[u8],
    stderr: &[u8],
    status: std::process::ExitStatus,
) -> Result<String> {
    let stdout = String::from_utf8_lossy(stdout);
    let mut order: Vec<String> = Vec::new();
    let mut parts: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut saw_event = false;

    for line in stdout.lines() {
        let line = line.trim();
        let Ok(event) = serde_json::from_str::<OpenCodeEvent>(line) else {
            // opencode also writes ANSI/banner noise to stdout. A real failure still arrives as an `error` event.
            continue;
        };
        saw_event = true;
        match event.r#type.as_str() {
            "error" => {
                let e = event.error.unwrap_or(OpenCodeError {
                    name: String::new(),
                    data: OpenCodeErrorData::default(),
                });
                let detail = if e.data.message.is_empty() {
                    e.name
                } else {
                    e.data.message
                };
                bail!("opencode CLI reported an error: {}", detail.trim());
            }
            "text" => {
                if let Some(part) = event.part {
                    if parts.insert(part.id.clone(), part.text).is_none() {
                        order.push(part.id);
                    }
                }
            }
            _ => {}
        }
    }

    if !saw_event {
        let stderr = String::from_utf8_lossy(stderr);
        bail!("opencode CLI exited with {status}: {}", stderr.trim());
    }

    let text: String = order
        .iter()
        .filter_map(|id| parts.get(id))
        .map(String::as_str)
        .collect();
    if text.is_empty() {
        bail!("opencode CLI returned no text output");
    }
    Ok(text)
}

/// Flatten a chat history into the single prompt argument. Each call is a fresh process, so the full history is sent every time.
fn flatten(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .map(|m| format!("[{}]\n{}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `$PATH` lookup for an executable file named `name`.
fn find_on_path(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(name);
        let meta = std::fs::metadata(&candidate).ok()?;
        (meta.is_file() && meta.permissions().mode() & 0o111 != 0).then_some(candidate)
    })
}

/// Unique suffix for a scratch dir name, from the process id plus a counter.
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

    /// Verbatim head of a live `cursor-agent --list-models` run plus a hypothetical prose footer.
    #[test]
    fn parse_model_list_takes_ids_only() {
        let stdout = "Available models\n\
                      \n\
                      auto - Auto (default)\n\
                      gpt-5.3-codex-low - Codex 5.3 Low\n\
                      composer-2.5-fast - Composer 2.5 Fast\n\
                      claude-opus-5-thinking-high - Claude Opus 5 1M Thinking\n\
                      \n\
                      Some note - about something else\n";
        assert_eq!(
            parse_model_list(stdout),
            vec![
                "auto",
                "gpt-5.3-codex-low",
                "composer-2.5-fast",
                "claude-opus-5-thinking-high",
            ]
        );
    }

    #[test]
    fn only_claude_lacks_a_model_list_arg() {
        assert_eq!(CliProgram::Claude.list_models_arg(), None);
        assert_eq!(
            CliProgram::CursorAgent.list_models_arg(),
            Some("--list-models")
        );
        assert_eq!(CliProgram::OpenCode.list_models_arg(), Some("models"));
    }

    #[tokio::test]
    async fn list_models_rejects_a_program_without_the_arg() {
        let err = list_models(CliProgram::Claude).await.unwrap_err();
        assert!(
            err.to_string().contains("no model-list command"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_on_path_locates_a_known_binary() {
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

    /// opencode's argv shares nothing with the other two: a `run` subcommand, `--format`, and the prompt as a trailing positional.
    #[test]
    fn build_args_uses_opencodes_own_shape() {
        let oc = CliClient {
            binary: PathBuf::from("opencode"),
            program: CliProgram::OpenCode,
            model: Some("minimax-coding-plan/MiniMax-M3".to_string()),
        };
        assert_eq!(
            oc.build_args("say hi"),
            vec![
                "run",
                "--format",
                "json",
                "--pure",
                "--model",
                "minimax-coding-plan/MiniMax-M3",
                "say hi",
            ]
        );
    }

    /// `--auto` auto-approves permissions and must never be used, because the prompt carries untrusted transcript text.
    #[test]
    fn build_args_never_auto_approves_opencode() {
        let oc = CliClient {
            binary: PathBuf::from("opencode"),
            program: CliProgram::OpenCode,
            model: None,
        };
        let args = oc.build_args("prompt");
        assert!(!args.contains(&"--auto".to_string()));
        assert!(args.contains(&"--pure".to_string()));
    }

    /// Without this env override opencode offers the model the user's global MCP servers (see `CliClient::env`).
    #[test]
    fn opencode_denies_every_tool_through_the_environment() {
        let oc = CliClient {
            binary: PathBuf::from("opencode"),
            program: CliProgram::OpenCode,
            model: None,
        };
        assert_eq!(oc.env(), &[("OPENCODE_PERMISSION", r#"{"*":"deny"}"#)]);

        // The other two carry their containment in argv.
        for program in [CliProgram::Claude, CliProgram::CursorAgent] {
            let c = CliClient {
                binary: PathBuf::from(program.binary_name()),
                program,
                model: None,
            };
            assert!(c.env().is_empty(), "{program:?} should need no env");
        }
    }

    /// Verbatim shape of a live `opencode models` run: bare `<provider>/<model>` lines with no label.
    #[test]
    fn parse_model_list_takes_bare_opencode_ids() {
        let stdout = "minimax-coding-plan/MiniMax-M3\n\
                      openrouter/anthropic/claude-sonnet-4.5\n\
                      \n\
                      deepseek/deepseek-v4-pro\n";
        assert_eq!(
            parse_model_list(stdout),
            vec![
                "minimax-coding-plan/MiniMax-M3",
                "openrouter/anthropic/claude-sonnet-4.5",
                "deepseek/deepseek-v4-pro",
            ]
        );
    }

    #[test]
    fn interpret_jsonl_returns_the_text_parts() {
        let stdout = concat!(
            r#"{"type":"step_start","part":{"id":"prt_1","type":"step-start"}}"#,
            "\n",
            r#"{"type":"reasoning","part":{"id":"prt_2","type":"reasoning","text":"thinking out loud"}}"#,
            "\n",
            r#"{"type":"text","part":{"id":"prt_3","type":"text","text":"391"}}"#,
            "\n",
            r#"{"type":"step_finish","part":{"id":"prt_4","type":"step-finish"}}"#,
            "\n",
        );
        let got =
            interpret_output(CliProgram::OpenCode, stdout.as_bytes(), b"", exit_status(0)).unwrap();
        // Reasoning is the model's thinking, not its reply.
        assert_eq!(got, "391");
    }

    /// A failed run exits non-zero having printed a structured error line and nothing on stderr.
    #[test]
    fn interpret_jsonl_surfaces_the_error_event_even_on_nonzero_exit() {
        let stdout = r#"{"type":"error","error":{"name":"UnknownError","data":{"message":"Unexpected server error. Check server logs for details."}}}"#;
        let err =
            match interpret_output(CliProgram::OpenCode, stdout.as_bytes(), b"", exit_status(1)) {
                Err(e) => e,
                Ok(_) => panic!("expected an error"),
            };
        assert_eq!(
            err.to_string(),
            "opencode CLI reported an error: Unexpected server error. Check server logs for details."
        );
    }

    /// A part streamed as successive snapshots must not repeat its prefix: the last snapshot wins and distinct ids still join.
    #[test]
    fn interpret_jsonl_keys_text_by_part_id() {
        let stdout = concat!(
            r#"{"type":"text","part":{"id":"prt_1","type":"text","text":"Hel"}}"#,
            "\n",
            r#"{"type":"text","part":{"id":"prt_1","type":"text","text":"Hello"}}"#,
            "\n",
            r#"{"type":"text","part":{"id":"prt_2","type":"text","text":" world"}}"#,
            "\n",
        );
        let got =
            interpret_output(CliProgram::OpenCode, stdout.as_bytes(), b"", exit_status(0)).unwrap();
        assert_eq!(got, "Hello world");
    }

    #[test]
    fn interpret_jsonl_falls_back_to_exit_status_when_stdout_has_no_events() {
        let err = match interpret_output(
            CliProgram::OpenCode,
            b"",
            b"error: unknown option '--not-a-real-flag'\n",
            exit_status(1),
        ) {
            Err(e) => e,
            Ok(_) => panic!("expected an error"),
        };
        assert!(err.to_string().contains("unknown option"));
        assert!(err.to_string().contains("opencode CLI exited with"));
    }

    #[test]
    fn interpret_jsonl_rejects_a_run_that_produced_no_text() {
        let stdout = r#"{"type":"step_finish","part":{"id":"prt_1","type":"step-finish"}}"#;
        let err =
            match interpret_output(CliProgram::OpenCode, stdout.as_bytes(), b"", exit_status(0)) {
                Err(e) => e,
                Ok(_) => panic!("expected an error"),
            };
        assert!(err.to_string().contains("no text output"), "{err}");
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

    /// `CliClient::stream` finishes generating before it yields, so a model that thinks for ~90 s must come back as a slow success, not a 60 s timeout.
    #[test]
    fn cli_budgets_for_generation_not_a_handshake() {
        let claude = CliClient {
            binary: PathBuf::from("claude"),
            program: CliProgram::Claude,
            model: Some("haiku".to_string()),
        };
        let budget = claude.response_timeout();
        assert!(
            budget > crate::HTTP_TIMEOUT,
            "CLI budget {budget:?} must exceed the HTTP handshake budget {:?}",
            crate::HTTP_TIMEOUT,
        );
        // The observed 94 s haiku run is the floor this has to clear.
        assert!(
            budget >= Duration::from_secs(180),
            "CLI budget {budget:?} is under the measured 94 s worst case",
        );
        // The inner subprocess guard must not fire before the caller gives up.
        assert!(
            CLI_TIMEOUT >= budget,
            "subprocess guard {CLI_TIMEOUT:?} must not preempt the caller's {budget:?}",
        );
    }

    /// The HTTP adapter keeps the tighter budget because `stream()` resolves at the response headers.
    #[test]
    fn http_keeps_the_handshake_budget() {
        let http = crate::OpenAiCompatClient::new(
            "https://example.test/v1".to_string(),
            "k".to_string(),
            "m".to_string(),
        );
        assert_eq!(http.response_timeout(), crate::HTTP_TIMEOUT);
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

    use std::os::unix::process::ExitStatusExt;

    fn exit_status(code: i32) -> std::process::ExitStatus {
        std::process::ExitStatus::from_raw(code)
    }

    /// A clean exit whose `result` is empty is still a failure.
    #[test]
    fn interpret_output_rejects_an_empty_result() {
        let stdout = r#"{"is_error":false,"result":"  \n"}"#;
        let err = match interpret_output(CliProgram::Claude, stdout.as_bytes(), b"", exit_status(0))
        {
            Err(e) => e,
            Ok(s) => panic!("expected an error, got {s:?}"),
        };
        assert_eq!(err.to_string(), "claude CLI returned no text output");
    }

    #[test]
    fn interpret_output_surfaces_the_json_error_even_on_nonzero_exit() {
        let stdout = r#"{"is_error":true,"result":"Not logged in · Please run /login"}"#;
        let err = match interpret_output(CliProgram::Claude, stdout.as_bytes(), b"", exit_status(1))
        {
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
        let got = interpret_output(CliProgram::Claude, stdout, b"", exit_status(0)).unwrap();
        assert_eq!(got, "PONG");
    }

    #[test]
    fn interpret_output_falls_back_to_exit_status_when_stdout_is_not_json() {
        // A crash before the CLI's own output formatting has no JSON to parse, so stderr and exit status are used.
        let err = match interpret_output(
            CliProgram::Claude,
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
        for program in CliProgram::ALL {
            assert_eq!(CliProgram::parse(program.binary_name()), Some(*program));
        }
        assert_eq!(CliProgram::parse("gpt-4o"), None);
        assert_eq!(CliProgram::parse(""), None);
    }

    #[test]
    fn locate_finds_a_program_actually_on_path() {
        if let Some(binary) = find_on_path(CliProgram::Claude.binary_name()) {
            let client = CliClient::locate(CliProgram::Claude, None).expect("claude is on PATH");
            assert_eq!(client.binary, binary);
            assert_eq!(client.program(), CliProgram::Claude);
        }
    }

    #[test]
    fn locate_names_the_program_when_not_found() {
        // Only meaningful when cursor-agent is not installed.
        if find_on_path(CliProgram::CursorAgent.binary_name()).is_none() {
            // `CliClient` isn't `Debug`, so `unwrap_err` won't compile.
            match CliClient::locate(CliProgram::CursorAgent, None) {
                Err(e) => assert!(e.to_string().contains("cursor-agent")),
                Ok(_) => panic!("expected an error when cursor-agent isn't on PATH"),
            }
        }
    }
}
