// YogurtEditor — the hero augmented-notes editor.
//
// Renders the swatch contract (CONTEXT D-31): user content ink-black, AI
// content grey, transcript deep-links lilac. The 660px max-width is
// load-bearing per PRD §5.3 / NOTES-01.
//
// Lifecycle responsibilities:
//   - `initialMarkdown` boots the editor doc on mount.
//   - `enrichedMarkdown` changes (post-meeting load, Re-enhance) replace
//     the content via `editor.commands.setContent(html, …)`.
//   - `onChange` fires after every doc mutation with the serialized
//     markdown (consumed by callers that want to persist incremental
//     edits — the in-meeting Notes panel).
//   - `onTranscriptLinkClick` is invoked from event delegation on the
//     editor DOM whenever the user activates a `↳ HH:MM` link (CONTEXT
//     D-32). The host (`MeetingPost.tsx`) wires this to open the
//     transcript dock and scroll to the timestamp.

import { useEditor, EditorContent } from "@tiptap/react";
import { useEffect, useRef } from "react";
import { yogurtExtensions } from "./extensions";
import { markdownToHtml, docToMarkdown } from "./markdown";

export interface YogurtEditorProps {
  initialMarkdown?: string;
  enrichedMarkdown?: string;
  editable?: boolean;
  onChange?: (markdown: string) => void;
  onTranscriptLinkClick?: (tsSeconds: number) => void;
  /** Extra className on the outer wrapper (test hook + theme overrides). */
  className?: string;
  /**
   * Real placeholder text (Meeting.tsx task 3) rendered via
   * `@tiptap/extension-placeholder` when the doc is empty — never enters
   * the document, so it can't leak into notes_md / the LLM prompt the way
   * the old seeded-content approach did.
   */
  placeholder?: string;
}

export function YogurtEditor(props: YogurtEditorProps) {
  const {
    initialMarkdown = "",
    enrichedMarkdown,
    editable = true,
    onChange,
    onTranscriptLinkClick,
    className,
    placeholder,
  } = props;

  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onLinkRef = useRef(onTranscriptLinkClick);
  onLinkRef.current = onTranscriptLinkClick;

  const editor = useEditor({
    extensions: yogurtExtensions({ placeholder }),
    editable,
    content: markdownToHtml(initialMarkdown),
    onUpdate: ({ editor }) => {
      if (!onChangeRef.current) return;
      try {
        onChangeRef.current(docToMarkdown(editor.state.doc));
      } catch {
        // Serialization should never throw, but if it does, swallow it —
        // an editor that crashes the host on every keystroke would be
        // catastrophic.
      }
    },
  });

  // Re-enhance / post-meeting load: replace content when enriched markdown
  // arrives. Skipped on first mount (handled by `content` above).
  useEffect(() => {
    if (!editor) return;
    if (enrichedMarkdown == null) return;
    const html = markdownToHtml(enrichedMarkdown);
    // `setContent(html, false)` to NOT emit an `onUpdate` (we don't want
    // the server-sent enriched doc to round-trip back through onChange).
    editor.commands.setContent(html, false);
  }, [editor, enrichedMarkdown]);

  // Click + keyboard event delegation for transcriptLink atoms.
  useEffect(() => {
    if (!editor) return;
    const dom = editor.view.dom;

    const handleActivate = (target: EventTarget | null) => {
      if (!(target instanceof HTMLElement)) return;
      const link = target.closest("[data-transcript-link]");
      if (!link) return;
      const raw = link.getAttribute("data-ts");
      const ts = Number(raw ?? "");
      if (!Number.isFinite(ts)) return;
      onLinkRef.current?.(ts);
    };

    const click = (e: MouseEvent) => handleActivate(e.target);
    const keydown = (e: KeyboardEvent) => {
      if (e.key !== "Enter" && e.key !== " ") return;
      handleActivate(e.target);
    };
    dom.addEventListener("click", click);
    dom.addEventListener("keydown", keydown);
    return () => {
      dom.removeEventListener("click", click);
      dom.removeEventListener("keydown", keydown);
    };
  }, [editor]);

  // 660px max-width is load-bearing per PRD §5.3 / NOTES-01.
  return (
    <div
      className={`yogurt-editor ${className ?? ""}`}
      style={{ maxWidth: "660px", margin: "0 auto" }}
      data-testid="yogurt-editor"
    >
      <EditorContent editor={editor} />
    </div>
  );
}
