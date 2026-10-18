import { type ButtonHTMLAttributes, type ReactNode } from "react";

type Variant = "primary" | "secondary" | "ghost" | "warn" | "on";

interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "className"> {
  variant?: Variant;
  children: ReactNode;
  className?: string;
}

const BASE = [
  "inline-flex",
  "items-center",
  "justify-center",
  "gap-2",
  "px-[18px]",
  "py-2.5",
  "rounded-button",
  "font-sans",
  "text-[14px]",
  "font-semibold",
  "leading-none",
  "transition-colors",
  "transition-shadow",
  "duration-150",
  "ease-out",
  "select-none",
  // Keyboard focus ring (WCAG 2.4.7). Blueberry at 40% alpha reads against both filled-blue and cream variants.
  "focus-visible:outline-none",
  "focus-visible:ring-2",
  "focus-visible:ring-blue/40",
  "focus-visible:ring-offset-2",
  "focus-visible:ring-offset-paper",
  "disabled:opacity-50",
  "disabled:cursor-not-allowed",
].join(" ");

const VARIANT: Record<Variant, string> = {
  // opacity-90 on hover: Tailwind 4 has no derived blue-600 shade without an explicit token.
  primary: "bg-blue text-white shadow-button-blue hover:opacity-90 active:opacity-100",
  secondary:
    "bg-card text-ink border border-line hover:bg-paper active:bg-line/60",
  ghost: "bg-transparent text-mut hover:text-ink hover:bg-blsoft/50",
  // The "needs noticing, not an error" tone; matches MetaPill's warn tone and the recording-error banner.
  warn: "bg-straw text-white hover:opacity-90 active:opacity-100",
  on: "bg-mtsoft text-matcha border border-matcha hover:opacity-90 active:opacity-100",
};

/** Variant classes for an element that must be an `<a>` (e.g. `x-apple.systempreferences:`) but should look like `<Button>`. */
export function buttonClassName(variant: Variant = "primary", extra = ""): string {
  return `${BASE} ${VARIANT[variant]} ${extra}`.trim();
}

export function Button({
  variant = "primary",
  children,
  className = "",
  type = "button",
  ...rest
}: ButtonProps) {
  return (
    <button type={type} className={buttonClassName(variant, className)} {...rest}>
      {children}
    </button>
  );
}
