/**
 * HI-03 — Tailwind 4 utility-existence smoke test.
 *
 * Asserting that `el.className` contains `bg-mut` only proves the string
 * is there — NOT that Tailwind actually emitted a CSS rule for it. If a
 * developer types `bg-mute` (typo) or a future @theme refactor drops a
 * token, the component class string still contains `bg-mut`, the regex
 * assertions pass, and the element silently has no background.
 *
 * This file closes that gap by reading the *built* CSS file from
 * `dist/assets/` and asserting every token-derived utility we depend on
 * actually exists in the bundle. It runs after `pnpm build` (or
 * `pnpm test` if a build has been done previously — `dist/` is a
 * checked-in build product for verification convenience).
 *
 * If you see this test fail, the most likely cause is one of:
 *   - a token was renamed in `index.css` `@theme` but a component still
 *     refers to the old utility (e.g. `bg-mut` after `--color-mut`
 *     was renamed to `--color-muted`);
 *   - a new utility (e.g. `text-foo`) was used in a component but its
 *     `--color-foo` token was never added to `@theme`;
 *   - the dist build is stale — re-run `pnpm build`.
 */
import { describe, it, expect } from "vitest";
import { readFileSync, existsSync, readdirSync } from "node:fs";
import { join } from "node:path";

const DIST_ASSETS = join(__dirname, "..", "..", "dist", "assets");

/**
 * All token-derived utility classes that Phase 1 components actually use
 * (verified by `pnpm build` output). If you add a new component that
 * references a new utility, add it here too — otherwise a typo in the
 * utility name will silently no-op and downstream phases won't catch it.
 *
 * NOTE: utilities consumed only as opacity-modified variants
 * (`border-matcha/25`, `border-straw/40`, …) are emitted by Tailwind 4
 * under different class names and are NOT listed here directly. Their
 * existence is implicitly verified by the .border-line / .border-blue /
 * .border-straw entries (Tailwind only emits opacity utilities if the
 * base color token is registered).
 */
const TOKEN_UTILITIES = [
  // bg-*
  ".bg-paper",
  ".bg-card",
  ".bg-blue",
  ".bg-blsoft",
  ".bg-mtsoft",
  ".bg-strsoft",
  ".bg-matcha",
  ".bg-straw",
  ".bg-mut",
  ".bg-ink",
  // text-*
  ".text-ink",
  ".text-mut",
  ".text-blue",
  ".text-matcha",
  ".text-straw",
  ".text-white",
  // border-* (only base forms used unmodified)
  ".border-line",
  ".border-blue",
  ".border-straw",
];

/**
 * Animation utilities derived from `--animate-*` tokens. Only the
 * animations referenced in actual JSX className strings are emitted by
 * Tailwind 4's JIT. `pop-up` and `slide-in-right` are tokenized in
 * `@theme` but currently consumed only by Phase 6 / Phase 3 (not yet
 * landed), so they intentionally are NOT asserted here — add them when
 * the respective phase wires the animation to runtime state.
 */
const ANIMATION_UTILITIES = [
  ".animate-recpulse",
  ".animate-blink",
  ".animate-shimmer",
  ".animate-wave",
  ".animate-float",
  ".animate-staggered-reveal",
];

function readBuiltCss(): string | null {
  if (!existsSync(DIST_ASSETS)) return null;
  const cssFile = readdirSync(DIST_ASSETS).find((f) => f.endsWith(".css"));
  if (!cssFile) return null;
  return readFileSync(join(DIST_ASSETS, cssFile), "utf-8");
}

describe("Tailwind 4 token utility existence (HI-03)", () => {
  const css = readBuiltCss();

  // The dist/ build may not exist in fresh checkouts; skip rather than
  // false-fail. CI is expected to run `pnpm build` before `pnpm test`.
  const maybeIt = css ? it : it.skip;

  for (const cls of TOKEN_UTILITIES) {
    maybeIt(`emits a CSS rule for ${cls}`, () => {
      // Escape regex specials and match `.bg-mut{...var(--color-mut)...}`.
      const escaped = cls.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      // Look for ".bg-mut{" — startoftoken + `{`.
      const ruleRe = new RegExp(`${escaped}\\s*\\{`);
      expect(css).toMatch(ruleRe);
    });
  }

  for (const cls of ANIMATION_UTILITIES) {
    maybeIt(`emits a CSS rule for ${cls}`, () => {
      const escaped = cls.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      const ruleRe = new RegExp(`${escaped}\\s*\\{`);
      expect(css).toMatch(ruleRe);
    });
  }

  maybeIt("text-mut resolves to var(--color-mut) (no silent drop)", () => {
    expect(css).toMatch(/\.text-mut\s*\{[^}]*var\(--color-mut\)[^}]*\}/);
  });

  maybeIt("bg-strsoft resolves to var(--color-strsoft) (no silent drop)", () => {
    expect(css).toMatch(/\.bg-strsoft\s*\{[^}]*var\(--color-strsoft\)[^}]*\}/);
  });

  maybeIt("animate-staggered-reveal references the staggered-reveal keyframe", () => {
    // Tailwind emits `.animate-staggered-reveal{animation:var(--animate-staggered-reveal)}`
    expect(css).toMatch(
      /\.animate-staggered-reveal\s*\{[^}]*var\(--animate-staggered-reveal\)[^}]*\}/,
    );
    // And the @keyframes block exists.
    expect(css).toMatch(/@keyframes\s+staggered-reveal\s*\{/);
  });
});
