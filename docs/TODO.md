# TODO

Open work for yogurt.
Add new items at the bottom of the relevant section, or create a new section.
Keep one item per `- [ ]` line so it's easy to check off.

## Referencing attachments

Drop screenshots and photos in `attachments/`, then reference them inline in the TODO entry.

When the user sends a photo, save it into `attachments/` with a `YYYY-MM-DD-<slug>.<ext>` filename, then add a TODO entry that links to it.

Example entry:

```
- [ ] Settings modal cuts off the API key field on small windows
  ![settings modal cutoff](attachments/2026-08-28-settings-modal-cutoff.png)
```

## UI

- [ ] Chat input loses its pill shape on focus
  The library search field stays pill-shaped when focused, with a lavender pill outline wrapping the input.
  The in-meeting "Ask this meeting..." input is also pill-shaped when collapsed, but once you click into it, the text field drops the pill background and renders as a standard rectangular input with a thick focus ring.
  It should match the search field - same pill shape on focus.

  Visual evidence:
  ![search field stays pill-shaped on focus](attachments/2026-08-28-search-focused-pill-baseline.png)
  ![chat input collapsed, pill correct](attachments/2026-08-28-chat-input-collapsed-pill.png)
  ![chat input focused, pill lost](attachments/2026-08-28-chat-input-focused-no-pill.png)

- [ ] Thick blue ring around the AI notes section when interacting
  In the in-meeting notes panel, typing into "your notes" leaves a heavy blue/purple border wrapped around the AI-generated content on the right.
  Same on post-meeting notes: clicking into the meeting summary section triggers the same thick ring around the whole AI area.
  Nothing is actually focused in those cases - it's a styling bug (likely `:focus-within` or panel-active state styling the wrong target). Remove or restyle so it doesn't read as a focus indicator.

  Visual evidence:
  ![notes AI section shows a thick focus ring](attachments/2026-08-28-notes-ai-section-focus-ring.png)

## Audio

- [ ] Add NVIDIA Parakeet v3 to the local STT model download
  The model registry at `crates/yogurt-stt/src/models.rs` only ships whisper.cpp checkpoints today (tiny.en, small.en, medium.en, large-v3), pulled by `scripts/refresh-model-hashes.sh`.
  Add Parakeet v3 as a downloadable local model - new `ModelSpec` entry, download URL, SHA256 pin, and the engine adapter if Parakeet can't reuse the whisper.cpp runtime.
  Heads-up: Parakeet is an NVIDIA NeMo checkpoint, not a ggml/gguf file - decide the engine first (NeMo ONNX export, whisper.cpp's Parakeet backend, or a new `yogurt-stt` engine next to `WhisperLocal`) before scoping this.