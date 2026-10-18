// The staggerMs values are 140 / 340 / 560 / 760 ms and the shimmer runs 1.25s linear infinite. Do not retune.

import { useEffect, useState } from "react";

export interface ShimmerSkeletonProps {
  staggerMs: number;
  widthClass?: string;
}

/** Unmounted on arrival of real content with a hard cut, no fade. */
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
