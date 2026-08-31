-- yogurt-db schema V009 - explicit CLI-backed LLM providers (LLM-4).
--
-- `adapter` distinguishes how a `kind='llm'` provider row is actually
-- driven: 'http' (OpenAiCompatClient against base_url + a stored API key,
-- the only shape that existed before this migration) or 'cli' (a local
-- agent CLI spawned by yogurt_llm::CliClient, no base_url or key
-- involved). For a 'cli' row, `model` is repurposed to hold the CLI's
-- program id ("claude" | "cursor-agent") instead of a model name, and
-- `base_url` is unused (empty string).
ALTER TABLE providers ADD COLUMN adapter TEXT NOT NULL DEFAULT 'http';
