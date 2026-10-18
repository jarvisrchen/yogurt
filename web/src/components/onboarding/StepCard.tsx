/** `ml-10` aligns the body with the title's left text edge (28px badge + gap-2). */

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
