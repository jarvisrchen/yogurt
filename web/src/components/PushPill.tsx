import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { hideCloudPushPill, useCloudPush } from "../hooks/useCloudPush";
import { TaskPill } from "./TaskPill";

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
  const pct =
    state.kind === "running" && state.total > 0
      ? Math.min(100, Math.round((state.sent / state.total) * 100))
      : null;

  return (
    <TaskPill
      kind={state.kind}
      pct={pct}
      text={text}
      label={label}
      onClick={onClick}
      closeLabel="Close the Cloud push notice"
      onClose={hideCloudPushPill}
      testId="push-pill"
    />
  );
}
