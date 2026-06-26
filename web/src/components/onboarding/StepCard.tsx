/**
 * Phase 7 Plan 07-04 — onboarding StepCard primitive (PRD §5.10).
 *
 * Three states, mirrored in both the border and the leading badge:
 *
 *   done    → matcha border, matcha badge with ✓
 *   current → blueberry 2px border, blueberry-soft badge with the number
 *   pending → line-color border, line-color badge with the number
 *
 * The body sits under the title and is `ml-10` so it lines up with the
 * title's leftmost text edge (badge is 28px + gap-2 = ~36px ≈ 10 of
 * tailwind's 4px steps). `children` slot is for optional inline content
 * like the provider chips in Step 2.
 */

import type { ReactNode } from "react";

export type StepState = "done" | "current" | "pending";

export interface StepCardProps {
  number: number;
  title: string;
  body: string;
  state: StepState;
  children?: ReactNode;
}

export function StepCard({
  number,
  title,
  body,
  state,
  children,
}: StepCardProps) {
  const borderClass =
    state === "done"
      ? "border border-matcha"
      : state === "current"
        ? "border-2 border-blue"
        : "border border-line";

  const badgeClass =
    state === "done"
      ? "bg-mtsoft text-matcha"
      : state === "current"
        ? "bg-blsoft text-blue"
        : "bg-line/50 text-mut";

  const badgeContent = state === "done" ? "✓" : String(number);

  return (
    <div
      className={`bg-card rounded-card shadow-card p-4 ${borderClass}`}
      data-state={state}
    >
      <div className="flex items-center gap-3">
        <div
          className={`w-7 h-7 rounded-full flex items-center justify-center text-[13px] font-semibold ${badgeClass}`}
          aria-hidden
        >
          {badgeContent}
        </div>
        <h3 className="font-sans text-[14px] font-semibold text-ink">
          {title}
        </h3>
      </div>
      <p className="text-[13px] text-mut ml-10 mt-1">{body}</p>
      {children ? <div className="ml-10 mt-3">{children}</div> : null}
    </div>
  );
}
