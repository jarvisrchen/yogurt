/**
 * Asserts the built CSS contains each token-derived utility. A className assertion
 * cannot catch a typo or a dropped `@theme` token. Reads `dist/assets/`, so build first.
 */
import { describe, it, expect } from "vitest";
import { readFileSync, existsSync, readdirSync } from "node:fs";
import { join } from "node:path";

const DIST_ASSETS = join(__dirname, "..", "..", "dist", "assets");

/**
 * Utilities used only as opacity variants (`border-matcha/25`) are not listed;
 * Tailwind emits them only when the base color token is registered, which the
 * `.border-line` / `.border-blue` / `.border-straw` entries already prove.
 */
const TOKEN_UTILITIES = [
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
  ".text-ink",
  ".text-mut",
  ".text-blue",
  ".text-matcha",
  ".text-straw",
  ".text-white",
  ".border-line",
  ".border-blue",
  ".border-straw",
];

/** `pop-up` and `slide-in-right` are not asserted until a component uses them (Tailwind emits only utilities that appear in JSX). */
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
      const escaped = cls.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
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
    expect(css).toMatch(
      /\.animate-staggered-reveal\s*\{[^}]*var\(--animate-staggered-reveal\)[^}]*\}/,
    );
    expect(css).toMatch(/@keyframes\s+staggered-reveal\s*\{/);
  });
});
