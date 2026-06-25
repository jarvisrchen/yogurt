import { type ButtonHTMLAttributes, type ReactNode } from "react";

type Variant = "primary" | "secondary" | "ghost";

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
  primary:
    "bg-blue text-white shadow-[0_2px_8px_rgba(91,79,199,0.3)] hover:opacity-90 active:opacity-100",
  // Cream card surface with a slightly warmer line color than --line, per §16.6.
  secondary:
    "bg-card text-ink border border-[#D9D0C0] hover:bg-[#F8F2E5] active:bg-[#F1E9D7]",
  // Transparent, used inside cards where any background would compete.
  ghost: "bg-transparent text-mut hover:text-ink hover:bg-blsoft/50",
};

/**
 * Yogurt brand button (PRD §16.6).
 *
 * Variants:
 *   - "primary"   — blueberry, white text. New meeting / End meeting / CTAs.
 *   - "secondary" — white card, ink text, hairline border. Cancel / Restart.
 *   - "ghost"     — transparent, muted text. Subtle dismissals.
 *
 * Future variants (deferred to Phase 4): "ink" — ink-on-cream End-meeting
 * style used in the live meeting top bar.
 */
export function Button({
  variant = "primary",
  children,
  className = "",
  type = "button",
  ...rest
}: ButtonProps) {
  const cls = `${BASE} ${VARIANT[variant]} ${className}`.trim();
  return (
    <button type={type} className={cls} {...rest}>
      {children}
    </button>
  );
}
