import { type ReactNode, type ElementType, type HTMLAttributes } from "react";

type Padding = "sm" | "md" | "lg";

/**
 * Extra props beyond the Card's own controls are forwarded to the
 * rendered element. Typed as a permissive HTMLAttributes bag so callers
 * can pass `onClick`, `role`, `aria-*`, etc. when using `as="button"`,
 * `as="a"`, etc. (MD-01)
 *
 * NOTE: this does NOT statically enforce required attributes per chosen
 * element (e.g. `type="button"` is not auto-applied when as="button").
 * Phase 5/7 audit when the first clickable card lands — explicitly pass
 * `type="button"` to avoid form-submit surprises.
 */
interface CardProps extends Omit<HTMLAttributes<HTMLElement>, "className"> {
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
 *
 * Polymorphic via `as`. All extra props (onClick, role, aria-*, …) are
 * spread onto the rendered element so `<Card as="button" onClick={…}>`
 * works. When using `as="button"`, callers should explicitly pass
 * `type="button"` to avoid implicit form-submit behavior. (MD-01)
 */
export function Card({
  children,
  active = false,
  padding = "md",
  as,
  className = "",
  ...rest
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

  return (
    <Tag className={cls} {...rest}>
      {children}
    </Tag>
  );
}
