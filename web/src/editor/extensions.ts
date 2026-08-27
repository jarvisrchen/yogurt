// TipTap extension bundle for the YogurtEditor.
//
// StarterKit baseline (configured per PRD §16.3 — headings 1-3 only) plus
// the two Phase 4 hero extensions: `aiGrey` mark and `transcriptLink`
// inline atom node.

import StarterKit from "@tiptap/starter-kit";
import Placeholder from "@tiptap/extension-placeholder";
import { AiGrey } from "./marks/aiGrey";
import { TranscriptLink } from "./marks/transcriptLink";

/**
 * `placeholder` renders via `@tiptap/extension-placeholder`'s
 * `is-editor-empty` CSS hook (styled in index.css's `.yogurt-editor` block)
 * — real placeholder text that never enters the document, unlike the old
 * Meeting.tsx approach of seeding the doc with literal prose (Meeting.tsx
 * task 3). An empty string (the default — MeetingPost never sets one)
 * renders nothing, so this is a no-op for hosts that don't need it.
 */
export function yogurtExtensions(opts: { placeholder?: string } = {}) {
  return [
    StarterKit.configure({ heading: { levels: [1, 2, 3] } }),
    Placeholder.configure({ placeholder: opts.placeholder ?? "" }),
    AiGrey,
    TranscriptLink,
  ];
}
