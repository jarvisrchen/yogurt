import StarterKit from "@tiptap/starter-kit";
import Placeholder from "@tiptap/extension-placeholder";
import { AiGrey } from "./marks/aiGrey";
import { TranscriptLink } from "./marks/transcriptLink";

// Placeholder text renders via the `is-editor-empty` CSS hook and never enters the document. An empty string renders nothing.
export function yogurtExtensions(opts: { placeholder?: string } = {}) {
  return [
    StarterKit.configure({ heading: { levels: [1, 2, 3] } }),
    Placeholder.configure({ placeholder: opts.placeholder ?? "" }),
    AiGrey,
    TranscriptLink,
  ];
}
