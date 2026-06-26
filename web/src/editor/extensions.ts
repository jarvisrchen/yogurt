// TipTap extension bundle for the YogurtEditor.
//
// StarterKit baseline (configured per PRD §16.3 — headings 1-3 only) plus
// the two Phase 4 hero extensions: `aiGrey` mark and `transcriptLink`
// inline atom node.

import StarterKit from "@tiptap/starter-kit";
import { AiGrey } from "./marks/aiGrey";
import { TranscriptLink } from "./marks/transcriptLink";

export function yogurtExtensions() {
  return [
    StarterKit.configure({ heading: { levels: [1, 2, 3] } }),
    AiGrey,
    TranscriptLink,
  ];
}
