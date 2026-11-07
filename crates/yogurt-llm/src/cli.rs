//! Local agent-CLI adapter: runs `claude -p`, `cursor-agent -p`, `opencode run`, `codex exec` or `copilot -p` as a one-shot completion backend.
//!
//! `yogurt-server` reaches this only when the active `providers` row is explicitly `adapter: "cli"`.
//! There is deliberately no automatic fallback from an unreachable HTTP provider to this,
//! because silently rerouting a meeting's content to a different backend is a change the user must opt into.
//!
//! `claude` and `cursor-agent` print one JSON object with `result` and `is_error`.
//! `opencode run --format json`, `codex exec --json` and `copilot --output-format json` print JSON lines instead, so [`interpret_output`] dispatches on the program.

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

/// `codex features list` names that give the model a tool. Each goes to `--disable`.
const CODEX_DISABLED_FEATURES: &[&str] = &[
    "code_mode_host",
    "shell_tool",
    "unified_exec",
    "unified_exec_tty",
    "multi_agent",
    "apps",
    "plugins",
    "remote_plugin",
    "browser_use",
    "browser_use_external",
    "browser_use_full_cdp_access",
    "in_app_browser",
    "computer_use",
    "image_generation",
    "view_image",
    "hooks",
    "skill_search",
    "skill_mcp_dependency_install",
    "tool_call_mcp_elicitation",
    "tool_suggest",
    "goals",
    "sleep_tool",
];

/// `COPILOT_ALLOW_ALL` auto-approves every tool and trusts the cwd's hooks and MCP servers.
/// `COPILOT_PROVIDER_BASE_URL` sends the prompt to a BYOK endpoint instead of the GitHub Copilot the user picked in Settings.
const COPILOT_STRIPPED_ENV: &[&str] = &["COPILOT_ALLOW_ALL", "COPILOT_PROVIDER_BASE_URL"];

/// Which agent CLI backs a [`CliClient`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliProgram {
    Claude,
    CursorAgent,
    OpenCode,
    Codex,
    Copilot,
}

impl CliProgram {
    /// Every variant, in preset order. The single place the set is enumerated.
    pub const ALL: &'static [CliProgram] = &[
        CliProgram::Claude,
        CliProgram::CursorAgent,
        CliProgram::OpenCode,
        CliProgram::Codex,
        CliProgram::Copilot,
    ];

    pub fn binary_name(self) -> &'static str {
        match self {
            CliProgram::Claude => "claude",
            CliProgram::CursorAgent => "cursor-agent",
            CliProgram::OpenCode => "opencode",
            CliProgram::Codex => "codex",
            CliProgram::Copilot => "copilot",
        }
    }

    /// Parse the id a `providers.model` row stores for a `cli`-adapter provider.
    pub fn parse(id: &str) -> Option<Self> {
        CliProgram::ALL
            .iter()
            .copied()
            .find(|p| p.binary_name() == id)
    }

    /// The arguments that make this CLI print its `--model` catalog and exit, or `None` if it has no such mode.
    ///
    /// `claude` has none: its four aliases ship as static preset suggestions.
    /// `copilot` has none either: its catalog only appears in a debug log, and its ACP `session/new` returns no model list.
    /// The others have ids that vary by account and plan.
    /// `codex debug models` is a debug command, so a Codex release may change or drop it.
    pub fn list_models_args(self) -> Option<&'static [&'static str]> {
        match self {
            CliProgram::Claude | CliProgram::Copilot => None,
            CliProgram::CursorAgent => Some(&["--list-models"]),
            CliProgram::OpenCode => Some(&["models"]),
            // `exec` ignores the user's config, so the catalog is pinned to the same default provider it uses.
            CliProgram::Codex => Some(&["debug", "models", "-c", r#"model_provider="openai""#]),
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
    let args = program
        .list_models_args()
        .ok_or_else(|| anyhow!("{} has no model-list command", program.binary_name()))?;
    let flag = args.join(" ");
    let binary = find_on_path(program.binary_name())
        .ok_or_else(|| anyhow!("{} not found on $PATH", program.binary_name()))?;

    let mut cmd = tokio::process::Command::new(&binary);
    cmd.kill_on_drop(true)
        .args(args)
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let models = match program {
        CliProgram::Codex => parse_codex_catalog(&stdout)?,
        _ => parse_model_list(&stdout),
    };
    if models.is_empty() {
        bail!("{} {flag} returned no models", program.binary_name());
    }
    Ok(models)
}

/// Pull model ids out of a plain-text catalog listing's stdout. Neither CLI offers JSON for it, and they print different lines:
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

#[derive(Deserialize)]
struct CodexCatalog {
    models: Vec<CodexModel>,
}

#[derive(Deserialize)]
struct CodexModel {
    slug: String,
    #[serde(default)]
    visibility: String,
}

/// `codex debug models` prints `{"models":[{"slug":..,"visibility":"list"|"hide",..}]}`.
/// Only `list` models appear in Codex's own picker; `hide` ones are internal (`codex-auto-review`) or unreleased.
fn parse_codex_catalog(stdout: &str) -> Result<Vec<String>> {
    let catalog: CodexCatalog = serde_json::from_str(stdout.trim())
        .context("codex debug models printed unexpected JSON")?;
    Ok(catalog
        .models
        .into_iter()
        .filter(|m| m.visibility == "list")
        .map(|m| m.slug)
        .collect())
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
    /// Fully per-program because the CLIs share no argv shape.
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
            // The user's `config.toml` can set `danger-full-access` and load MCP servers (Gmail, Granola), so it is ignored outright.
            // Disabling `code_mode_host` makes the exec tool fail closed: asked to read a file outside the scratch cwd, the model answers it cannot.
            // The other features are account-level tools (ChatGPT apps, browser, computer use) that `--ignore-user-config` does not drop.
            // `~/.codex/AGENTS.md` still loads and no flag skips it. A scratch `CODEX_HOME` would, but a token refresh there rotates the user's real refresh token.
            // Never `--dangerously-bypass-approvals-and-sandbox` or `--approve-for-me`: the input is untrusted.
            CliProgram::Codex => {
                let mut args: Vec<String> = [
                    "exec",
                    "--json",
                    "--ephemeral",
                    "--skip-git-repo-check",
                    "--ignore-user-config",
                    "--ignore-rules",
                    "--sandbox",
                    "read-only",
                    "-c",
                    "include_apply_patch_tool=false",
                    "-c",
                    r#"web_search="disabled""#,
                    "-c",
                    "skills.include_instructions=false",
                    "-c",
                    "tools.experimental_request_user_input.enabled=false",
                    // Same reason as claude's `--effort low`: these are extraction calls.
                    "-c",
                    r#"model_reasoning_effort="low""#,
                ]
                .map(str::to_string)
                .into();
                for feature in CODEX_DISABLED_FEATURES {
                    args.push("--disable".to_string());
                    args.push(feature.to_string());
                }
                args
            }
            // `--available-tools` is an allowlist and no tool is named `none`, so the model sees no tools at all, its built-in GitHub MCP tools included.
            // The `--deny-tool` kinds deny every permission request even if a later release turns that filter open.
            // `--disable-builtin-mcps` keeps the GitHub MCP server, which holds the user's GitHub credential, from even connecting.
            // `--no-auto-update` runs the installed version, since each call's scratch `COPILOT_HOME` would otherwise download a fresh update.
            // No `--reasoning-effort`: copilot rejects it outright for `auto` and for models without effort levels.
            // Never `--allow-all`, `--yolo`, `--allow-all-tools` or `--trust-folder`: the input is untrusted.
            CliProgram::Copilot => [
                "-p",
                prompt,
                "--output-format",
                "json",
                "--available-tools=none",
                "--deny-tool=shell",
                "--deny-tool=write",
                "--deny-tool=read",
                "--deny-tool=url",
                "--deny-tool=memory",
                "--disable-builtin-mcps",
                "--no-custom-instructions",
                "--no-ask-user",
                "--sync-session=off",
                "--no-auto-update",
                "--disable-client-telemetry",
            ]
            .map(str::to_string)
            .into(),
        };
        if let Some(model) = &self.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }
        // opencode and codex take the prompt as a trailing positional, after every flag.
        // It cannot look like a flag because `flatten` always prefixes a `[role]` line.
        if matches!(self.program, CliProgram::OpenCode | CliProgram::Codex) {
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
            CliProgram::Claude
            | CliProgram::CursorAgent
            | CliProgram::Codex
            | CliProgram::Copilot => &[],
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
        if self.program == CliProgram::Copilot {
            let home = match isolated_copilot_home(&scratch, user_copilot_home()) {
                Ok(home) => home,
                Err(e) => {
                    let _ = std::fs::remove_dir_all(&scratch);
                    return Err(e);
                }
            };
            cmd.env("COPILOT_HOME", home);
            for var in COPILOT_STRIPPED_ENV {
                cmd.env_remove(var);
            }
        }

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
        CliProgram::Codex => interpret_codex_jsonl(stdout, stderr, status),
        CliProgram::Copilot => interpret_copilot_jsonl(stdout, stderr, status),
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

/// One line of `codex exec --json` output.
/// A top-level `error` line is not fatal on its own because Codex prints one per retry, so it only explains a turn that never completes.
#[derive(Deserialize)]
struct CodexEvent {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    item: Option<CodexItem>,
    #[serde(default)]
    error: Option<CodexError>,
    /// Set on a top-level `error` line.
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
struct CodexItem {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct CodexError {
    #[serde(default)]
    message: String,
}

/// `codex exec --json`: JSON lines, with the same stdout-before-exit-status rule as [`interpret_single_json`].
///
/// The model may send a progress note before its answer, so the last `agent_message` is the reply, the same one `--output-last-message` writes.
/// It counts only once `turn.completed` arrives: a process that dies mid-turn must not hand back that progress note as the answer.
/// An error message is often the API's own JSON error body, so its inner `error.message` is unwrapped when present.
fn interpret_codex_jsonl(
    stdout: &[u8],
    stderr: &[u8],
    status: std::process::ExitStatus,
) -> Result<String> {
    let stdout = String::from_utf8_lossy(stdout);
    let mut last_message: Option<String> = None;
    let mut last_error: Option<String> = None;

    for line in stdout.lines() {
        let Ok(event) = serde_json::from_str::<CodexEvent>(line.trim()) else {
            continue;
        };
        match event.r#type.as_str() {
            "turn.failed" => {
                let raw = event.error.map(|e| e.message).unwrap_or_default();
                bail!("codex CLI reported an error: {}", codex_error_detail(&raw));
            }
            "error" => last_error = event.message,
            "item.completed" => {
                if let Some(item) = event.item.filter(|i| i.r#type == "agent_message") {
                    last_message = Some(item.text);
                }
            }
            "turn.completed" => {
                return match last_message {
                    Some(text) if !text.trim().is_empty() => Ok(text),
                    _ => bail!("codex CLI returned no text output"),
                };
            }
            _ => {}
        }
    }

    if let Some(raw) = last_error {
        bail!("codex CLI reported an error: {}", codex_error_detail(&raw));
    }
    let stderr = String::from_utf8_lossy(stderr);
    bail!("codex CLI exited with {status}: {}", stderr.trim());
}

/// One line of `copilot --output-format json` output.
/// Only `assistant.message` and the closing `result` matter. The rest are session bookkeeping.
#[derive(Deserialize)]
struct CopilotEvent {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    data: Option<CopilotMessage>,
    /// Set on the closing `result` line.
    #[serde(rename = "exitCode")]
    exit_code: Option<i32>,
}

#[derive(Deserialize)]
struct CopilotMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    phase: String,
}

/// `copilot --output-format json`: JSON lines, ending in a `result` line with the exit code.
///
/// A message in the `commentary` phase is a progress note, so the reply is the last other `assistant.message`.
/// It counts only once a `result` line with exit code 0 arrives.
/// A failure (bad model, not logged in) prints no `result` line, exits 1, and explains itself on stderr as `Error: ...`.
/// A `result` line with a nonzero exit code falls through to that same stderr.
fn interpret_copilot_jsonl(
    stdout: &[u8],
    stderr: &[u8],
    status: std::process::ExitStatus,
) -> Result<String> {
    let stdout = String::from_utf8_lossy(stdout);
    let mut last_message: Option<String> = None;

    for line in stdout.lines() {
        let Ok(event) = serde_json::from_str::<CopilotEvent>(line.trim()) else {
            continue;
        };
        match event.r#type.as_str() {
            "assistant.message" => {
                if let Some(msg) = event.data.filter(|m| m.phase != "commentary") {
                    last_message = Some(msg.content);
                }
            }
            "result" if event.exit_code == Some(0) => {
                return match last_message {
                    Some(text) if !text.trim().is_empty() => Ok(text),
                    _ => bail!("copilot CLI returned no text output"),
                };
            }
            _ => {}
        }
    }

    let stderr = String::from_utf8_lossy(stderr);
    let detail = stderr.trim();
    if let Some(message) = detail.strip_prefix("Error: ") {
        bail!("copilot CLI reported an error: {message}");
    }
    if detail.is_empty() {
        bail!("copilot CLI exited with {status} and printed no error");
    }
    bail!("copilot CLI exited with {status}: {detail}")
}

fn codex_error_detail(raw: &str) -> String {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| raw.trim().to_string())
}

/// The user's copilot state dir: `$COPILOT_HOME`, else `~/.copilot`.
fn user_copilot_home() -> Option<PathBuf> {
    std::env::var_os("COPILOT_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".copilot")))
}

/// A fresh `COPILOT_HOME` under `scratch` holding only a copy of the user's `config.json`, which records their login.
///
/// The user's own home would hand the call their MCP servers (`mcp-config.json`), saved tool approvals, skills and plugins,
/// and copilot would keep the prompt in its session history there for good.
/// The scratch copy goes when `scratch` does.
fn isolated_copilot_home(scratch: &std::path::Path, user_home: Option<PathBuf>) -> Result<PathBuf> {
    let home = scratch.join(".copilot-home");
    std::fs::create_dir(&home).with_context(|| format!("failed to create {}", home.display()))?;
    if let Some(config) = user_home
        .map(|h| h.join("config.json"))
        .filter(|p| p.is_file())
    {
        std::fs::copy(&config, home.join("config.json"))
            .with_context(|| format!("failed to copy {}", config.display()))?;
    }
    Ok(home)
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
    fn claude_and_copilot_lack_a_model_list_command() {
        assert_eq!(CliProgram::Claude.list_models_args(), None);
        assert_eq!(CliProgram::Copilot.list_models_args(), None);
        assert_eq!(
            CliProgram::CursorAgent.list_models_args(),
            Some(&["--list-models"][..])
        );
        assert_eq!(
            CliProgram::OpenCode.list_models_args(),
            Some(&["models"][..])
        );
        assert_eq!(
            CliProgram::Codex.list_models_args(),
            Some(&["debug", "models", "-c", r#"model_provider="openai""#][..])
        );
    }

    /// Trimmed from a live `codex debug models` run on codex-cli 0.155.1.
    #[test]
    fn parse_codex_catalog_keeps_listed_models_only() {
        let stdout = r#"{"models":[
            {"slug":"gpt-6-astra","display_name":"GPT-6 Astra","visibility":"list","priority":1},
            {"slug":"gpt-reserve","visibility":"hide"},
            {"slug":"gpt-5.6-luna","visibility":"list"},
            {"slug":"codex-auto-review","visibility":"hide"}
        ]}"#;
        assert_eq!(
            parse_codex_catalog(stdout).unwrap(),
            vec!["gpt-6-astra", "gpt-5.6-luna"]
        );
        assert!(parse_codex_catalog("not json").is_err());
    }

    /// Every flag that hands the model a tool or the user's permissive config must stay off, and no bypass flag may appear.
    #[test]
    fn build_args_locks_codex_down() {
        let codex = CliClient {
            binary: PathBuf::from("codex"),
            program: CliProgram::Codex,
            model: Some("gpt-5.6-luna".to_string()),
        };
        let args = codex.build_args("[user]\nhi");
        assert_eq!(args[0], "exec");
        for flag in [
            "--json",
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
        ] {
            assert!(args.contains(&flag.to_string()), "missing {flag}");
        }
        let sandbox = args.iter().position(|a| a == "--sandbox").unwrap();
        assert_eq!(args[sandbox + 1], "read-only");
        for feature in [
            "code_mode_host",
            "shell_tool",
            "apps",
            "plugins",
            "computer_use",
        ] {
            assert!(
                args.windows(2)
                    .any(|w| w[0] == "--disable" && w[1] == feature),
                "{feature} not disabled"
            );
        }
        assert!(!args
            .iter()
            .any(|a| a.contains("dangerously") || a == "--approve-for-me"));
        let model = args.iter().position(|a| a == "--model").unwrap();
        assert_eq!(args[model + 1], "gpt-5.6-luna");
        assert_eq!(args.last().unwrap(), "[user]\nhi");
    }

    /// Verbatim shape of a live run whose model sent a progress note before answering.
    #[test]
    fn interpret_codex_jsonl_returns_the_last_agent_message() {
        let stdout = concat!(
            r#"{"type":"thread.started","thread_id":"01a1"}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"item_0","type":"error","message":"Code Mode is unavailable because code-mode host is disabled."}}"#,
            "\n",
            r#"{"type":"turn.started"}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"Checking the transcript."}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"391"}}"#,
            "\n",
            r#"{"type":"turn.completed","usage":{"input_tokens":22947,"output_tokens":104}}"#,
            "\n",
        );
        let got =
            interpret_output(CliProgram::Codex, stdout.as_bytes(), b"", exit_status(0)).unwrap();
        assert_eq!(got, "391");
    }

    /// Verbatim from a live run with an unknown `--model`: exit 1, the reason only inside `turn.failed`.
    #[test]
    fn interpret_codex_jsonl_unwraps_a_failed_turn() {
        let stdout = concat!(
            r#"{"type":"turn.started"}"#,
            "\n",
            r#"{"type":"error","message":"{\"type\":\"error\",\"status\":400,\"error\":{\"type\":\"invalid_request_error\",\"message\":\"The 'not-a-model' model is not supported when using Codex with a ChatGPT account.\",\"code\":\"model_not_found\"}}"}"#,
            "\n",
            r#"{"type":"turn.failed","error":{"message":"{\"type\":\"error\",\"status\":400,\"error\":{\"type\":\"invalid_request_error\",\"message\":\"The 'not-a-model' model is not supported when using Codex with a ChatGPT account.\",\"code\":\"model_not_found\"}}"}}"#,
            "\n",
        );
        let err = interpret_output(CliProgram::Codex, stdout.as_bytes(), b"", exit_status(1))
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "codex CLI reported an error: The 'not-a-model' model is not supported when using Codex with a ChatGPT account."
        );
    }

    /// A process that dies after a progress note, with no `turn.completed`, must not return that note as the answer.
    #[test]
    fn interpret_codex_jsonl_rejects_a_turn_that_never_completed() {
        let stdout = concat!(
            r#"{"type":"turn.started"}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"Checking the transcript."}}"#,
            "\n",
            r#"{"type":"error","message":"stream disconnected before completion"}"#,
            "\n",
        );
        let err = interpret_output(CliProgram::Codex, stdout.as_bytes(), b"", exit_status(1))
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "codex CLI reported an error: stream disconnected before completion"
        );

        let no_error_line = r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"Checking."}}"#;
        let err = interpret_output(
            CliProgram::Codex,
            no_error_line.as_bytes(),
            b"killed",
            exit_status(1),
        )
        .unwrap_err();
        assert!(err.to_string().contains("killed"), "{err}");
    }

    #[test]
    fn interpret_codex_jsonl_falls_back_to_stderr_without_events() {
        let err = interpret_output(
            CliProgram::Codex,
            b"",
            b"Error finding codex home",
            exit_status(1),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("Error finding codex home"),
            "{err}"
        );
    }

    #[test]
    fn build_args_locks_copilot_down() {
        let copilot = CliClient {
            binary: PathBuf::from("copilot"),
            program: CliProgram::Copilot,
            model: Some("auto".to_string()),
        };
        let args = copilot.build_args("[user]\nhi");
        assert_eq!(args[..2], ["-p", "[user]\nhi"]);
        for flag in [
            "--available-tools=none",
            "--deny-tool=shell",
            "--deny-tool=write",
            "--deny-tool=read",
            "--deny-tool=url",
            "--deny-tool=memory",
            "--disable-builtin-mcps",
            "--no-custom-instructions",
            "--no-ask-user",
            "--sync-session=off",
            "--no-auto-update",
            "--disable-client-telemetry",
        ] {
            assert!(args.contains(&flag.to_string()), "missing {flag}");
        }
        assert!(!args.iter().any(|a| a.starts_with("--allow")
            || a == "--yolo"
            || a == "--trust-folder"
            || a.starts_with("--reasoning-effort")));
        let model = args.iter().position(|a| a == "--model").unwrap();
        assert_eq!(args[model + 1], "auto");
    }

    /// Trimmed from a live copilot 1.0.107 run: a progress note, then the answer, then `result`.
    #[test]
    fn interpret_copilot_jsonl_skips_the_progress_note() {
        let stdout = r###"{"type":"session.mcp_server_status_changed","data":{"serverName":"github-mcp-server","status":"disabled"},"ephemeral":true}
{"type":"assistant.turn_start","data":{"turnId":"0"}}
{"type":"assistant.message","data":{"content":"I'll read the notes first.","phase":"commentary","toolRequests":[]}}
{"type":"assistant.message","data":{"content":"## Decisions\n- Ship it","phase":"final_answer","toolRequests":[]}}
{"type":"assistant.turn_end","data":{"turnId":"0"}}
{"type":"result","sessionId":"72f3","exitCode":0}
"###;
        let text =
            interpret_output(CliProgram::Copilot, stdout.as_bytes(), b"", exit_status(0)).unwrap();
        assert_eq!(
            text,
            "## Decisions
- Ship it"
        );
    }

    /// Seen live: the turn ends on a progress note, so there is no answer to return.
    #[test]
    fn interpret_copilot_jsonl_rejects_a_turn_with_only_a_progress_note() {
        let stdout = r#"{"type":"assistant.message","data":{"content":"I'll check the directory.","phase":"commentary"}}
{"type":"result","exitCode":0}
"#;
        let err = interpret_output(CliProgram::Copilot, stdout.as_bytes(), b"", exit_status(0))
            .unwrap_err();
        assert_eq!(err.to_string(), "copilot CLI returned no text output");
    }

    /// A process that dies mid-turn prints no `result` line, so its message is not an answer.
    #[test]
    fn interpret_copilot_jsonl_rejects_a_run_without_a_result() {
        let stdout =
            r#"{"type":"assistant.message","data":{"content":"partial","phase":"final_answer"}}"#;
        let err = interpret_output(CliProgram::Copilot, stdout.as_bytes(), b"", exit_status(9))
            .unwrap_err();
        assert!(
            err.to_string().starts_with("copilot CLI exited with"),
            "{err}"
        );
    }

    #[test]
    fn interpret_copilot_jsonl_names_a_silent_failure() {
        let stdout = r#"{"type":"result","exitCode":1}"#;
        let err = interpret_output(
            CliProgram::Copilot,
            stdout.as_bytes(),
            b"",
            exit_status(256),
        )
        .unwrap_err();
        assert!(err.to_string().ends_with("and printed no error"), "{err}");
    }

    #[test]
    fn isolated_copilot_home_carries_only_the_login_config() {
        let root = std::env::temp_dir().join(format!("yogurt-copilot-home-test-{}", uuid_v4_ish()));
        let user = root.join("user");
        let scratch = root.join("scratch");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::write(user.join("config.json"), "{\"login\":1}").unwrap();
        std::fs::write(user.join("mcp-config.json"), "{}").unwrap();

        let home = isolated_copilot_home(&scratch, Some(user.clone())).unwrap();
        assert!(home.starts_with(&scratch));
        let mut files: Vec<_> = std::fs::read_dir(&home)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        files.sort();
        assert_eq!(files, ["config.json"]);

        // A user who has never run copilot has no config to carry.
        let bare = root.join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        let home = isolated_copilot_home(&bare, Some(root.join("missing"))).unwrap();
        assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Verbatim stderr of a live run with an unavailable `--model`.
    #[test]
    fn interpret_copilot_jsonl_surfaces_the_stderr_error() {
        let err = interpret_output(
            CliProgram::Copilot,
            b"",
            b"Error: Model \"gpt-6-luna\" from --model flag is not available.\n",
            exit_status(1),
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"copilot CLI reported an error: Model "gpt-6-luna" from --model flag is not available."#
        );
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

        // The others carry their containment in argv.
        for program in [
            CliProgram::Claude,
            CliProgram::CursorAgent,
            CliProgram::Codex,
            CliProgram::Copilot,
        ] {
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
