-- yogurt-db schema V010 - model override for CLI-backed LLM providers.
--
-- `cli_model` is the `--model` value passed to a `cli`-adapter row's agent
-- CLI (e.g. "sonnet" | "opus" | "haiku" for claude), separate from `model`
-- (which holds the CliProgram id, "claude" | "cursor-agent"). Empty string
-- means "use the CLI's own default model". Meaningless for `http` rows.
ALTER TABLE providers ADD COLUMN cli_model TEXT NOT NULL DEFAULT '';
