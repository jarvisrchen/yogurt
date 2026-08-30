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
  /**
   * Phase 5 (enhance streaming): when true, stamps `data-streaming` on the
   * outer wrapper so `web/src/index.css` can style the whole preview as AI
   * text with a blinking caret on the last block.
   */
  streaming?: boolean;
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
    streaming = false,
  } = props;

  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onLinkRef = useRef(onTranscriptLinkClick);
  onLinkRef.current = onTranscriptLinkClick;
  // Markdown this editor last emitted via `onChange`. Hosts keep that
  // string in state and pass it straight back as `enrichedMarkdown`
  // (MeetingPost's My notes / Enhanced tabs), so without this guard every
  // keystroke round-trips doc -> markdown -> setContent, which trims the
  // trailing space you just typed and resets the caret to the end.
  const lastEmittedRef = useRef<string | null>(null);

  const editor = useEditor({
    extensions: yogurtExtensions({ placeholder }),
    editable,
    content: markdownToHtml(initialMarkdown),
    onUpdate: ({ editor }) => {
      if (!onChangeRef.current) return;
      try {
        const md = docToMarkdown(editor.state.doc);
        lastEmittedRef.current = md;
        onChangeRef.current(md);
      } catch {
        // Serialization should never throw, but if it does, swallow it —
        // an editor that crashes the host on every keystroke would be
        // catastrophic.
      }
    },
  });

  // Streaming preview (Phase 5): `editable` can flip false to true / true to
  // false after mount (read-only while the raw preview streams in, editable
  // again once the final document lands) - TipTap's `useEditor` only
  // applies the `editable` option at creation time, so without this the
  // editor would stay stuck in whatever mode it was created with.
  useEffect(() => {
    // `setEditable`'s second arg defaults to `true` (it emits an `onUpdate`
    // even though the document didn't change) - pass `false` for the same
    // reason `setContent(html, false)` does below: a programmatic
    // editable-mode swap must never mark the host's dirty-edit flags.
    editor?.setEditable(editable, false);
  }, [editor, editable]);

  // Re-enhance / post-meeting load: replace content when enriched markdown
  // arrives. Skipped on first mount (handled by `content` above).
  useEffect(() => {
    if (!editor) return;
    if (enrichedMarkdown == null) return;
    if (enrichedMarkdown === lastEmittedRef.current) return;
    lastEmittedRef.current = null;
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
      {...(streaming ? { "data-streaming": "" } : {})}
    >
      <EditorContent editor={editor} />
    </div>
  );
}
