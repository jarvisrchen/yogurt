import { type ReactNode, type ElementType } from "react";

type Padding = "sm" | "md" | "lg";

interface CardProps {
  children: ReactNode;
  /** Wraps the card in a 1.5px blueberry border for "current step" / active states. */
  active?: boolean;
  /** Padding scale (4-base): sm=12, md=20, lg=32. */
  padding?: Padding;
  /** Element to render as. Defaults to <div>. Use 'article' for meeting cards. */
  as?: ElementType;
  className?: string;
}

const PADDING: Record<Padding, string> = {
  sm: "p-3",
  md: "p-5",
  lg: "p-8",
};

/**
 * Card — white 14px-radius surface with the standard card elevation.
 * Composes the §16.4 elevation token and §16.4 radius token.
 *
 * Use `active` for the "current step" visual in onboarding (§5.10) and the
 * active-provider card in settings (§5.6). Use `padding="lg"` for hero
 * onboarding cards, "md" for settings rows, "sm" for compact meeting cards.
 */
export function Card({
  children,
  active = false,
  padding = "md",
  as,
  className = "",
}: CardProps) {
  const Tag = (as ?? "div") as ElementType;
  const border = active ? "border-[1.5px] border-blue" : "border border-line";
  const cls = [
    "bg-card",
    "rounded-card",
    "shadow-card",
    border,
    PADDING[padding],
    className,
  ]
    .join(" ")
    .trim();

  return <Tag className={cls}>{children}</Tag>;
}
