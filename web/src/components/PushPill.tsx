import clsx from "clsx";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { hideCloudPushPill, useCloudPush } from "../hooks/useCloudPush";
import { ProgressRing } from "./ProgressRing";

const n = (v: number) => v.toLocaleString("en-US");

/** Whole seconds left until `at`, redrawn once a second. */
function useSecondsLeft(at: number | null): number | null {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (at == null) return;
    setNow(Date.now());
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, [at]);
  return at == null ? null : Math.max(0, Math.ceil((at - now) / 1000));
}

export function PushPill() {
  const push = useCloudPush();
  const navigate = useNavigate();
  const retryAt = push?.state.kind === "running" ? push.state.retryAt : null;
  const retryIn = useSecondsLeft(retryAt);
  if (!push || push.pillHidden) return null;
  const { state } = push;

  const open = () => {
    hideCloudPushPill();
    navigate("/settings/cloud");
  };
  const [label, text, onClick] =
    state.kind === "running"
      ? [
          "Show Cloud push progress",
          retryIn != null
            ? `Retrying in ${retryIn}s`
            : `Pushing to Cloud · ${n(state.sent)} of ${n(state.total)}`,
          () => navigate("/settings/cloud"),
        ]
      : state.kind === "done"
        ? [`Pushed ${n(state.pushed)} meetings, open Cloud sync`, `Pushed ${n(state.pushed)}`, open]
        : ["Show Cloud push error", "Push failed", open];
  const failed = state.kind === "failed";
  const pct =
    state.kind === "running" && state.total > 0
      ? Math.min(100, Math.round((state.sent / state.total) * 100))
      : null;

  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      data-testid="push-pill"
      className={clsx(
        "flex items-center gap-2 px-3 py-1.5 bg-card border rounded-full shadow-pop text-[13px] text-ink whitespace-nowrap transition-colors",
        failed ? "border-straw" : "border-line hover:border-matcha",
      )}
    >
      {state.kind === "running" ? (
        <ProgressRing pct={pct} />
      ) : (
        <span
          aria-hidden="true"
          className={clsx(
            "inline-flex w-3.5 justify-center font-semibold",
            failed ? "text-straw" : "text-matcha",
          )}
        >
          {failed ? "!" : "✓"}
        </span>
      )}
      <span className={clsx("font-medium", failed && "text-straw")}>{text}</span>
    </button>
  );
}
