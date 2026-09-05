import { type ButtonHTMLAttributes, type ReactNode } from "react";

type Variant = "primary" | "secondary" | "ghost" | "warn" | "on";

interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "className"> {
  variant?: Variant;
  children: ReactNode;
  /** Optional extra Tailwind classes appended after the variant classes. */
  className?: string;
}

const BASE = [
  "inline-flex",
  "items-center",
  "justify-center",
  "gap-2",
  "px-4",
  "py-2",
  "rounded-button",
  "font-sans",
  "text-[13.5px]",
  "font-semibold",
  "leading-none",
  "transition-colors",
  "transition-shadow",
  "duration-150",
  "ease-out",
  "select-none",
  // Visible focus ring for keyboard users (WCAG 2.4.7 / MD-03).
  // Uses the blueberry token at 40% alpha so the ring reads against
  // primary (filled-blue) and secondary (cream) variants equally.
  "focus-visible:outline-none",
  "focus-visible:ring-2",
  "focus-visible:ring-blue/40",
  "focus-visible:ring-offset-2",
  "focus-visible:ring-offset-paper",
  "disabled:opacity-50",
  "disabled:cursor-not-allowed",
].join(" ");

const VARIANT: Record<Variant, string> = {
  // Blueberry button with branded shadow. Hover darkens via opacity overlay
  // since Tailwind 4 doesn't expose a 'blue-600'-style derived shade without
  // explicit token; opacity-90 on hover is consistent with the design board.
  primary: "bg-blue text-white shadow-button-blue hover:opacity-90 active:opacity-100",
  // Card surface with a slightly stronger line than --line, per §16.6.
  secondary:
    "bg-card text-ink border border-line hover:bg-paper active:bg-line/60",
  // Transparent, used inside cards where any background would compete.
  ghost: "bg-transparent text-mut hover:text-ink hover:bg-blsoft/50",
  // Strawberry — the app's one "attention, but not an error" tone (matches
  // MetaPill's warn tone and the recording-error banner). For a state that
  // needs noticing, not a destructive action.
  warn: "bg-straw text-white hover:opacity-90 active:opacity-100",
  // Matcha "live" state - an active action, not an error or CTA.
  on: "bg-mtsoft text-matcha border border-matcha hover:opacity-90 active:opacity-100",
};

/**
 * Yogurt brand button (PRD §16.6).
 *
 * Variants:
 *   - "primary"   — blueberry, white text. New meeting / End meeting / CTAs.
 *   - "secondary" — white card, ink text, hairline border. Cancel / Restart.
 *   - "ghost"     — transparent, muted text. Subtle dismissals.
 *   - "warn"      — strawberry, white text. A state that needs noticing
 *                   (mic muted), not a destructive action.
 *
 * Future variants (deferred to Phase 4): "ink" — ink-on-cream End-meeting
 * style used in the live meeting top bar.
 */
/**
 * The full class string for a variant, for the rare element that must be
 * an `<a>` (e.g. an `x-apple.systempreferences:` link) but should look
 * exactly like `<Button>`.
 */
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
