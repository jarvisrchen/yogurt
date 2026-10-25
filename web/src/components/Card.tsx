import { type ReactNode, type ElementType, type HTMLAttributes } from "react";

type Padding = "sm" | "md" | "lg" | "compact";

/** Extra props spread onto the rendered element. Not enforced per element: pass `type="button"` explicitly with `as="button"` to avoid form-submit surprises. */
interface CardProps extends Omit<HTMLAttributes<HTMLElement>, "className"> {
  children: ReactNode;
  active?: boolean;
  padding?: Padding;
  as?: ElementType;
  className?: string;
}

const PADDING: Record<Padding, string> = {
  sm: "p-3",
  md: "p-5",
  lg: "p-8",
  compact: "px-[18px] py-4",
};

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
