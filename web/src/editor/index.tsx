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
  className?: string;
  /** Placeholder text never enters the document, so it cannot leak into notes_md or the LLM prompt. */
  placeholder?: string;
  /** Stamps `data-streaming` so index.css styles the preview as AI text with a blinking caret. */
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
  // Markdown last emitted via `onChange`. Hosts pass it back as `enrichedMarkdown`, and without this guard
  // every keystroke round-trips through setContent, trimming the trailing space and resetting the caret.
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
        // Serialization should never throw, but a crash on every keystroke would take down the host.
      }
    },
  });

  // useEditor applies `editable` only at creation, so sync later flips (read-only while the preview streams).
  useEffect(() => {
    // Pass false so a programmatic mode swap does not emit onUpdate and mark the host dirty.
    editor?.setEditable(editable, false);
  }, [editor, editable]);

  // First mount is handled by `content` above.
  useEffect(() => {
    if (!editor) return;
    if (enrichedMarkdown == null) return;
    if (enrichedMarkdown === lastEmittedRef.current) return;
    lastEmittedRef.current = null;
    const html = markdownToHtml(enrichedMarkdown);
    // false: the server-sent doc must not round-trip back through onChange.
    editor.commands.setContent(html, false);
  }, [editor, enrichedMarkdown]);

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

  // 660px is the PRD note column width.
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
