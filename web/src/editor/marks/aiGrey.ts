// Promote-on-edit: an `appendTransaction` plugin strips the mark from text inserted inside an `aiGrey` run,
// leaving the surrounding grey untouched. It over-promotes rather than under-promotes.
// The `ai-grey` name predates the `--color-ai` token that now styles it.

import { Mark, mergeAttributes } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

export const AiGreyPluginKey = new PluginKey("aiGreyPromote");

export const AiGrey = Mark.create<{}, { transcriptTs: number | null }>({
  name: "aiGrey",

  addAttributes() {
    return {
      transcriptTs: {
        default: null,
        parseHTML: (el) => {
          const raw = (el as HTMLElement).getAttribute("data-ts");
          if (raw == null) return null;
          const n = Number(raw);
          return Number.isFinite(n) ? n : null;
        },
        renderHTML: (attrs) => {
          if (attrs.transcriptTs == null) return {};
          return { "data-ts": String(attrs.transcriptTs) };
        },
      },
    };
  },

  parseHTML() {
    return [{ tag: "span[data-ai-grey]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, { "data-ai-grey": "", class: "ai-grey" }),
      0,
    ];
  },

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: AiGreyPluginKey,
        appendTransaction: (transactions, _oldState, newState) => {
          if (!transactions.some((t) => t.docChanged)) return null;

          const aiGreyType = newState.schema.marks.aiGrey;
          if (!aiGreyType) return null;

          let tr = newState.tr;
          let modified = false;

          for (const t of transactions) {
            if (!t.docChanged) continue;
            // TipTap stamps `preventUpdate` on programmatic `setContent(html, false)` swaps (load, re-enhance,
            // tab switch). Those are not user edits, and without this skip every grey run is promoted to ink on load.
            if (t.getMeta("preventUpdate")) continue;
            t.mapping.maps.forEach((stepMap) => {
              stepMap.forEach((_oldStart, _oldEnd, newStart, newEnd) => {
                if (newStart === newEnd) return;
                // Pure structural changes (e.g. wrapping) must not promote.
                let hasAiGrey = false;
                newState.doc.nodesBetween(newStart, newEnd, (node) => {
                  if (node.marks.some((m) => m.type === aiGreyType)) {
                    hasAiGrey = true;
                    return false;
                  }
                  return true;
                });
                if (!hasAiGrey) return;
                tr = tr.removeMark(newStart, newEnd, aiGreyType);
                modified = true;
              });
            });
          }

          return modified ? tr : null;
        },
      }),
    ];
  },
});
