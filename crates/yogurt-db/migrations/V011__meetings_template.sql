-- yogurt-db schema V011 - per-meeting note format.
--
-- Which enhance template shaped `enriched_md` (`yogurt_prompts::TEMPLATE_IDS`,
-- e.g. "standup"). Stamped by enhance.rs after a successful enhance with
-- the id the model picked, or the one the user forced. Nullable so rows
-- enhanced before this migration read back as NULL / None.
ALTER TABLE meetings ADD COLUMN template TEXT;
