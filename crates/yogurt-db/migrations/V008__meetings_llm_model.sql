-- yogurt-db schema V008 - per-meeting LLM provenance.
--
-- Which model produced `enriched_md`. Stamped by enhance.rs after a
-- successful enhance with the model name the resolved client actually
-- called (env override, active provider row, or "mock"). Nullable so
-- rows enhanced before this migration read back as NULL / None.
ALTER TABLE meetings ADD COLUMN llm_model TEXT;
