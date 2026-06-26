// ShimmerSkeleton — load-bearing motion primitive per PRD §16.5 + CONTEXT
// D-29 / D-30.
//
// While the LLM is enriching, AI bullets are painted as shimmer placeholders
// that resolve into grey markdown one at a time. The four canonical
// `staggerMs` values are 140 / 340 / 560 / 760 ms — these are NOT
// negotiable per the felt-quality contract (see PRD §16.5 motion tokens
// and Phase 4 acceptance gate item 5).
//
// The shimmer animation itself runs at exactly 1.25s linear infinite,
// driven by the Phase 1 `--animate-shimmer` token (web/src/index.css:57).
// We attach it via the `.shimmer` class rule below — Phase 1 only defined
// the token + keyframes; this file adds the actual class that consumes
// the gradient.

import { useEffect, useState } from "react";

export interface ShimmerSkeletonProps {
  /**
   * Milliseconds to wait after mount before showing the shimmer. PRD §16.5
   * specifies the four-bullet stagger as 140 / 340 / 560 / 760.
   */
  staggerMs: number;
  /** Tailwind width class. Defaults to `w-3/4` so bullets vary nicely. */
  widthClass?: string;
}

/**
 * Renders nothing for the first `staggerMs` ms, then a 20px-tall shimmer
 * bar. Once the actual content arrives the parent simply unmounts the
 * skeleton — there is no "fade out" step (the real text appears in its
 * place, hard cut, which reads as "the AI finished this bullet").
 */
export function ShimmerSkeleton({
  staggerMs,
  widthClass = "w-3/4",
}: ShimmerSkeletonProps) {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const t = setTimeout(() => setVisible(true), staggerMs);
    return () => clearTimeout(t);
  }, [staggerMs]);

  if (!visible) {
    // Reserve the row height so the layout doesn't jump when the shimmer
    // appears.
    return <div className={`h-5 ${widthClass}`} aria-hidden="true" />;
  }

  return (
    <div
      className={`h-5 ${widthClass} rounded shimmer`}
      role="status"
      aria-label="generating bullet"
      data-testid="shimmer-skeleton"
      data-stagger-ms={staggerMs}
    />
  );
}
