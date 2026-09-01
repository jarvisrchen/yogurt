// `aiGrey` TipTap mark — applied to runs the LLM added during enhance.
//
// Wire format (CONTEXT D-06): `<span data-ai-grey data-ts="N" class="ai-grey">…</span>`
//
// CSS contract: `.ai-grey { color: var(--color-ai); }` - the `--color-ai`
// token (`#7A7264` light / `#A69D90` dark, UI-7). The mark and class keep
// their "grey" name; only the token behind them moved.
//
// Promote-on-edit (CONTEXT D-04 / NOTES-10): an `appendTransaction` plugin
// watches every transaction; for each text-insertion range that lands
// inside an `aiGrey` mark, the mark is stripped from the inserted range
// only — surrounding grey is untouched. The conservative fallback noted in
// the superpowers plan: if a transaction lacks discriminating meta we
// still strip on insertion (better to over-promote than under-promote).

import { Mark, mergeAttributes } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

export interface AiGreyAttrs {
  transcriptTs?: number;
}

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
          // Only act on transactions that actually changed the doc.
          if (!transactions.some((t) => t.docChanged)) return null;

          const aiGreyType = newState.schema.marks.aiGrey;
          if (!aiGreyType) return null;

          let tr = newState.tr;
          let modified = false;

          for (const t of transactions) {
            if (!t.docChanged) continue;
            // TipTap stamps `preventUpdate` on its programmatic
            // `setContent(html, false)` replace (post-meeting load,
            // Re-enhance, tab switch). That is a whole-document swap, not
            // a user edit - without this skip the replaced range "carries
            // aiGrey" and every grey run gets promoted to ink on load.
            if (t.getMeta("preventUpdate")) continue;
            t.mapping.maps.forEach((stepMap) => {
              stepMap.forEach((_oldStart, _oldEnd, newStart, newEnd) => {
                if (newStart === newEnd) return;
                // Only strip if the inserted range actually carries aiGrey
                // marks in the new state — pure structural mutations
                // (e.g. wrapping) shouldn't promote.
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
