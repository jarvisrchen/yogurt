import { type ButtonHTMLAttributes, type ReactNode } from "react";

type Variant = "primary" | "secondary" | "ghost" | "warn" | "on";
type Size = "md" | "sm";

interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "className"> {
  variant?: Variant;
  size?: Size;
  children: ReactNode;
  className?: string;
}

const BASE = [
  "inline-flex",
  "items-center",
  "justify-center",
  "gap-2",
  "rounded-button",
  "font-sans",
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

const SIZE: Record<Size, string> = {
  md: "px-[18px] py-2.5 text-[14px] font-semibold leading-none",
  sm: "border px-3.5 py-2 text-[12.5px] font-bold leading-normal",
};

const VARIANT: Record<Variant, string> = {
  // opacity-90 on hover: Tailwind 4 has no derived blue-600 shade without an explicit token.
  primary: "bg-blue border-blue text-white shadow-button-blue hover:opacity-90 active:opacity-100",
  secondary:
    "bg-card text-ink border border-line hover:bg-paper active:bg-line/60",
  ghost: "bg-transparent border-transparent text-mut hover:text-ink hover:bg-blsoft/50",
  // The "needs noticing, not an error" tone; matches MetaPill's warn tone and the recording-error banner.
  warn: "bg-straw border-straw text-white hover:opacity-90 active:opacity-100",
  on: "bg-mtsoft text-matcha border border-matcha hover:opacity-90 active:opacity-100",
};

/** Variant classes for an element that must be an `<a>` (e.g. `x-apple.systempreferences:`) but should look like `<Button>`. */
export function buttonClassName(variant: Variant = "primary", extra = "", size: Size = "md"): string {
  return `${BASE} ${SIZE[size]} ${VARIANT[variant]} ${extra}`.trim();
}

export function Button({
  variant = "primary",
  size = "md",
  children,
  className = "",
  type = "button",
  ...rest
}: ButtonProps) {
  return (
    <button type={type} className={buttonClassName(variant, className, size)} {...rest}>
      {children}
    </button>
  );
}
