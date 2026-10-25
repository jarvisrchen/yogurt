import type { ReactNode } from "react";
import { Lock, TriangleAlert } from "lucide-react";

type Tone = "honey" | "matcha" | "straw";

const TONE: Record<Tone, string> = {
  honey: "bg-honeysoft text-honey",
  matcha: "bg-mtsoft text-matcha",
  // Same treatment as the Settings load-failure box.
  straw: "bg-strsoft text-ink border border-straw/40",
};

export function Note({
  tone,
  children,
  className = "",
  role,
}: {
  tone: Tone;
  children: ReactNode;
  className?: string;
  role?: "alert" | "status";
}) {
  return (
    <div
      role={role}
      className={`flex items-start gap-2 rounded-button px-3 py-[9px] text-[12px] ${TONE[tone]} ${className}`}
    >
      {tone === "matcha" ? (
        <Lock size={15} strokeWidth={1.6} className="mt-px shrink-0" aria-hidden />
      ) : (
        <TriangleAlert size={15} strokeWidth={1.6} className="mt-px shrink-0" aria-hidden />
      )}
      <div className="min-w-0">{children}</div>
    </div>
  );
}
