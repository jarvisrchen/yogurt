import type { ReactNode } from "react";
import { LABEL } from "../labelClass";

// Sizes live in separate constants and are never combined with a second `text-[..]` class, because Tailwind orders same-property utilities by its own sheet order and not by className order.
const INPUT_BASE =
  "block w-full rounded-button border bg-paper px-[11px] py-2 text-ink placeholder:text-grey focus:border-blue focus:outline-none disabled:cursor-not-allowed";
export const INPUT = `${INPUT_BASE} text-[12.5px]`;
export const INPUT_MONO = `${INPUT_BASE} font-mono text-[11.5px] placeholder:font-sans`;

export function Field({
  label,
  htmlFor,
  hint,
  children,
}: {
  label: string;
  htmlFor: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="mb-3 flex flex-col gap-[5px]">
      <label htmlFor={htmlFor} className={LABEL}>
        {label}
      </label>
      {children}
      {hint && <span className="text-[11.5px] text-mut">{hint}</span>}
    </div>
  );
}
