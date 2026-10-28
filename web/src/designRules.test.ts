import { describe, expect, it } from "vitest";

const sources = import.meta.glob(["./**/*.{ts,tsx,css}", "!./**/*.test.*"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const offenders = (pattern: RegExp, keep: (match: string) => boolean = () => true) =>
  Object.entries(sources).flatMap(([file, src]) =>
    [...src.matchAll(pattern)].map((m) => m[0]).filter(keep).map((m) => `${file}: ${m}`),
  );

describe("typography", () => {
  it("never sets a label in mono, which was the pre-Manrope eyebrow", () => {
    const classStrings = /["'`][^"'`\n]*\bfont-mono\b[^"'`\n]*["'`]/g;
    expect(offenders(classStrings, (s) => /\buppercase\b|tracking-\[/.test(s))).toEqual([]);
  });

  it("picks families only through the font-sans and font-mono tokens", () => {
    expect(offenders(/fontFamily\s*:|\bfont-\[|font-serif|@fontsource\/(?!manrope|jetbrains-mono)[\w-]+/g)).toEqual([]);
  });
});
