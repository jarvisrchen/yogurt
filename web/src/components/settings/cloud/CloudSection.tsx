import { useEffect, useState } from "react";
import { TriangleAlert } from "lucide-react";
import { Navigate, useNavigate } from "react-router";
import { Button, buttonClassName } from "../../Button";
import { Card } from "../../Card";
import { Segmented } from "../../Segmented";
import { Pill } from "../../Pill";
import {
  CloudError,
  cloudApi,
  useCloudMutation,
  useCloudStatus,
  type CloudConnected,
  type LastPush,
  type PushHistoryEntry,
  type PushStatus,
  type PushInterval,
} from "../../../lib/api/cloud";
import { useLivePush } from "../../../hooks/useCloudPush";
import { ConnectForm } from "./ConnectForm";
import { INPUT, K } from "./Field";
import { Note } from "./Note";
import { QrCode } from "./QrCode";

const n = (v: number) => v.toLocaleString("en-US");

const GUIDE_URL = "https://github.com/jarvisrchen/yogurt/blob/main/docs/CLOUD-SYNC.md";

function Header({ suffix, connected, intro }: { suffix?: string; connected?: boolean; intro: React.ReactNode }) {
  return (
    <header>
      <div className="flex flex-wrap items-baseline gap-2.5">
        <h2 className="heading-section">Cloud sync</h2>
        {suffix && <small className="font-mono text-[10.5px] font-medium text-mut">{suffix}</small>}
        {connected && (
          <Pill tone="matcha" className="px-2.5! py-[3px]! text-[11px]! font-bold!">
            Connected
          </Pill>
        )}
      </div>
      <p className="mb-4 mt-1 text-[13px] text-ai">{intro}</p>
    </header>
  );
}

function Choice({ title, lede, items, children }: { title: string; lede: string; items: string[]; children: React.ReactNode }) {
  return (
    <Card padding="compact" className="flex flex-col">
      <h3 className="heading-sm mb-1.5">{title}</h3>
      <p className="text-[12.5px] text-mut">{lede}</p>
      <ul className="mb-3.5 mt-1 list-disc pl-4 text-[12.5px] text-ai">
        {items.map((i) => (
          <li key={i}>{i}</li>
        ))}
      </ul>
      <div>{children}</div>
    </Card>
  );
}

function NotSetUp() {
  const navigate = useNavigate();
  return (
    <section>
      <Header
        suffix="optional · your own Vercel and Supabase"
        intro="Push your meetings to a Yogurt Cloud you deployed, then read them on your phone. This laptop only pushes and never reads them back. Nothing leaves this Mac until you push."
      />
      <div className="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-3">
        <Choice
          title="Create a Yogurt Cloud"
          lede="First time. Opens the setup guide in your browser."
          items={["A Vercel account", "A Supabase account", "The email you will sign in with"]}
        >
          <a href={GUIDE_URL} target="_blank" rel="noreferrer" className={buttonClassName("primary", "", "sm")}>
            Open the guide ↗
          </a>
        </Choice>
        <Choice title="Connect to a Yogurt Cloud" lede="Already deployed." items={["Your Cloud address", "Your Cloud login"]}>
          <Button variant="secondary" size="sm" onClick={() => navigate("/settings/cloud/connect")}>
            Connect
          </Button>
        </Choice>
      </div>
    </section>
  );
}

/** `10/23/26 2:59 PM`, in the user's local time. */
export function formatPushTime(ms: number): string {
  const d = new Date(ms);
  const date = d.toLocaleDateString("en-US", { month: "numeric", day: "numeric", year: "2-digit" });
  const time = d.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
  return `${date} ${time}`;
}

function counts(changed: number, deleted: number): string {
  const parts = [changed > 0 && `${n(changed)} uploaded`, deleted > 0 && `${n(deleted)} deleted`];
  return parts.filter(Boolean).join(", ") || "up to date";
}

export function describeLastPush(p: LastPush | null): string {
  if (!p) return "Not pushed yet";
  return `Last push ${formatPushTime(p.at)} · ${counts(p.changed, p.deleted)}`;
}

/** The failures since the last success, newest first. */
function failuresSince(history: PushHistoryEntry[]) {
  const i = history.findIndex((e) => e.outcome !== "failed");
  return history.slice(0, i === -1 ? history.length : i);
}

const TRIGGER: Record<PushHistoryEntry["trigger"], string> = {
  manual: "Manual",
  auto: "Auto",
  meeting: "Meeting",
  disconnect: "Disconnect",
};

export function describeEntry(e: PushHistoryEntry): string {
  if (e.outcome === "failed") return `Failed: ${e.error}`;
  if (e.outcome === "up_to_date") return "Up to date";
  return counts(e.changed, e.deleted);
}

function Marker({ e }: { e: PushHistoryEntry }) {
  const tone = e.outcome === "pushed" ? "bg-matcha" : e.outcome === "failed" ? "bg-straw" : "bg-mut";
  return (
    <span className="flex h-[1lh] w-3 shrink-0 items-center justify-center" aria-hidden>
      {e.outcome === "failed" && e.needs_you ? (
        <TriangleAlert size={12} strokeWidth={2} className="text-straw" />
      ) : (
        <span className={`size-1.5 rounded-full ${tone}`} />
      )}
    </span>
  );
}

export function LastPushLine({ last, history }: { last: LastPush | null; history: PushHistoryEntry[] }) {
  const [open, setOpen] = useState(false);
  const failed = failuresSince(history);
  const needsYou = failed[0]?.outcome === "failed" && failed[0].needs_you;
  return (
    <div className="text-[11.5px] text-mut">
      <div className="flex flex-wrap items-center gap-x-1.5 gap-y-1">
        {needsYou && <TriangleAlert size={12} strokeWidth={2} className="shrink-0 text-straw" aria-hidden />}
        <span>
          {describeLastPush(last)}
          {failed.length > 0 && (
            <span className="font-semibold text-straw">
              {" "}
              · {n(failed.length)} failed{last ? " since" : ""}
            </span>
          )}
        </span>
        {history.length > 0 && (
          <button
            type="button"
            aria-expanded={open}
            onClick={() => setOpen((o) => !o)}
            className="ml-auto cursor-pointer rounded font-semibold text-blue hover:underline focus-visible:outline-2 focus-visible:outline-blue"
          >
            History {open ? "▴" : "▾"}
          </button>
        )}
      </div>
      {open && (
        <ul aria-label="Recent pushes" className="mt-2.5 flex flex-col gap-1.5 border-t border-line pt-2.5">
          {history.map((e) => (
            <li key={`${e.at}-${e.outcome}`} className="flex min-w-0 gap-2.5">
              <Marker e={e} />
              <span className="w-[108px] shrink-0 tabular-nums">{formatPushTime(e.at)}</span>
              <span className="w-[62px] shrink-0">{TRIGGER[e.trigger]}</span>
              <span
                title={e.outcome === "failed" ? e.error : undefined}
                className={`min-w-0 truncate ${e.outcome === "failed" ? "text-straw" : "text-ink"}`}
              >
                {describeEntry(e)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** Stays up until a sign-in succeeds, so a wrong password does not dismiss the prompt. */
function SignInAgain({ onSignedIn }: { onSignedIn: () => void }) {
  const signIn = useCloudMutation(cloudApi.signIn);
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const submit = async () => {
    setError(null);
    try {
      await signIn.mutateAsync(password);
      onSignedIn();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setPassword("");
      signIn.reset();
    }
  };
  return (
    <form
      className="mt-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (password) void submit();
      }}
    >
      <Note tone="straw" role="alert">
        Your Cloud session expired. Sign in again to keep pushing.
      </Note>
      <div className="mt-2 flex items-center gap-2">
        <label htmlFor="cloud-resign" className="sr-only">
          Cloud login password
        </label>
        <input
          id="cloud-resign"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder="Cloud login password"
          autoComplete="off"
          className={`${INPUT} min-w-0 flex-1 border-line`}
        />
        <Button type="submit" size="sm" disabled={!password || signIn.isPending}>
          {signIn.isPending ? "Signing in…" : "Sign in"}
        </Button>
      </div>
      {error && (
        <p role="alert" className="mt-1.5 text-[11.5px] text-straw">
          {error}
        </p>
      )}
    </form>
  );
}

/** The running push's counts and bar, or its retry countdown. */
export function PushProgress({ push }: { push: PushStatus }) {
  const pct = push.total > 0 ? Math.min(100, Math.round((push.sent / push.total) * 100)) : 0;
  const waiting = push.retry_in != null;
  return (
    <div className="mb-3" data-testid="push-progress">
      <div className="mb-1 flex items-baseline justify-between gap-2 text-[12.5px]">
        <b>{push.total > 0 ? `Pushing ${n(push.sent)} of ${n(push.total)}` : "Preparing to push…"}</b>
        {waiting && <span className="text-mut">Retrying in {push.retry_in}s</span>}
      </div>
      <div
        role="progressbar"
        aria-label="Cloud push progress"
        aria-valuemin={0}
        aria-valuemax={push.total}
        aria-valuenow={push.sent}
        className="h-1.5 overflow-hidden rounded-full bg-mtsoft"
      >
        <div className="h-full rounded-full bg-matcha transition-[width] duration-200" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}

function Stat({ value, label, tone }: { value: number; label: string; tone?: string }) {
  return (
    <div className="flex flex-col">
      <b className={`text-[18px] font-extrabold ${tone ?? ""}`}>{n(value)}</b>
      <span className="text-[11px] text-mut">{label}</span>
    </div>
  );
}

const INTERVAL_OPTIONS: { value: PushInterval; label: string }[] = [
  { value: "off", label: "Off" },
  { value: "meeting", label: "After meetings" },
  { value: "15m", label: "15 min" },
  { value: "1h", label: "Hourly" },
  { value: "1d", label: "Daily" },
];

function AutoPush({ value }: { value: PushInterval }) {
  const set = useCloudMutation(cloudApi.setPushInterval);
  return (
    <Card padding="compact" className="mt-3">
      <div className="flex flex-wrap items-center justify-between gap-2.5">
        <div className="min-w-0">
          <div className={K}>Auto-push</div>
          <p className="mt-0.5 text-[11.5px] text-mut">
            {value === "meeting"
              ? "Push a minute after each meeting ends or its notes finish, and once a day for later edits, while yogurt is running."
              : "Push changed meetings in the background while yogurt is running. Off by default."}
          </p>
        </div>
        <Segmented
          label="Auto-push"
          size="sm"
          options={INTERVAL_OPTIONS}
          value={set.isPending ? set.variables : value}
          onChange={(v) => set.mutate(v)}
        />
      </div>
      {set.isError && (
        <Note tone="straw" role="alert" className="mt-3">
          {set.error.message}
        </Note>
      )}
    </Card>
  );
}

function Disconnect() {
  const disconnect = useCloudMutation(cloudApi.disconnect);
  const [open, setOpen] = useState(false);
  const [markDeleted, setMarkDeleted] = useState(false);
  if (!open) {
    return (
      <div className="mt-3.5 flex flex-wrap items-center justify-between gap-2.5">
        <span className="text-[11.5px] text-mut">
          Disconnect removes this cloud from this laptop. You can first run one last push that marks this
          machine&apos;s meetings deleted. History keeps them either way.
        </span>
        <Button variant="secondary" size="sm" className="text-straw! border-strsoft!" onClick={() => setOpen(true)}>
          Disconnect…
        </Button>
      </div>
    );
  }
  return (
    <Card padding="compact" className="mt-3.5">
      <h3 className="heading-sm mb-1">Disconnect from this Yogurt Cloud?</h3>
      <p className="text-[12.5px] text-mut">
        This laptop stops pushing and forgets the connection. Meetings already in the cloud stay there.
      </p>
      <label className="mt-2.5 flex items-center gap-2 text-[12.5px]">
        <input
          type="checkbox"
          checked={markDeleted}
          onChange={(e) => setMarkDeleted(e.target.checked)}
          className="accent-blue"
        />
        First mark this machine&apos;s meetings deleted, so the current library drops them
      </label>
      {disconnect.isError && (
        <Note tone="straw" role="alert" className="mt-3">
          {disconnect.error.message}
        </Note>
      )}
      <div className="mt-3.5 flex items-center justify-end gap-2.5">
        <Button variant="ghost" size="sm" onClick={() => setOpen(false)} disabled={disconnect.isPending}>
          Cancel
        </Button>
        <Button variant="warn" size="sm" disabled={disconnect.isPending} onClick={() => disconnect.mutate(markDeleted)}>
          {disconnect.isPending ? "Disconnecting…" : "Disconnect"}
        </Button>
      </div>
    </Card>
  );
}

function Connected({ status }: { status: CloudConnected }) {
  const push = useCloudMutation(cloudApi.push);
  const host = status.address.replace(/^https?:\/\//, "");
  // A dead session can surface from a push, from the disconnect's last push, or from the stored last error.
  // Once seen, the sign-in prompt stays until a sign-in succeeds.
  const sawExpired =
    status.last_error?.code === "session_expired" ||
    (push.error instanceof CloudError && push.error.code === "session_expired");
  const [needsSignIn, setNeedsSignIn] = useState(false);
  useEffect(() => {
    if (sawExpired) setNeedsSignIn(true);
  }, [sawExpired]);
  const livePush = useLivePush(status.push);
  const running = livePush != null;
  const notPushed = livePush && livePush.total > 0 ? Math.max(0, livePush.total - livePush.sent) : status.not_pushed;
  const error = needsSignIn || running ? null : (push.error?.message ?? status.last_error?.error);
  return (
    <section>
      <Header connected intro={`Signed in as ${status.email}`} />
      <Card active padding="compact" className="border-[1px]! ring-1 ring-blue">
        <div className="flex flex-wrap items-center justify-between gap-2.5">
          <div>
            <div className={K}>Machine name</div>
            <b className="text-[15px]">{status.machine_name}</b>
          </div>
          <Button size="sm" disabled={push.isPending || running || needsSignIn} onClick={() => push.mutate(undefined)}>
            {push.isPending || running ? "Pushing…" : "Push meetings"}
          </Button>
        </div>
        <div className="mb-3.5 mt-2.5 flex flex-wrap gap-6">
          <Stat value={status.meetings} label="meetings on this Mac" />
          <Stat value={notPushed} label="not pushed" tone={notPushed > 0 ? "text-honeyink" : ""} />
        </div>
        {livePush && <PushProgress push={livePush} />}
        {!(running && !status.last_push) && (
          <LastPushLine last={status.last_push} history={status.push_history ?? []} />
        )}
        {error && (
          <Note tone="straw" role="alert" className="mt-3">
            {error}
          </Note>
        )}
        {needsSignIn && (
          <SignInAgain
            onSignedIn={() => {
              setNeedsSignIn(false);
              push.reset();
            }}
          />
        )}
      </Card>
      <AutoPush value={status.push_interval} />
      <Card padding="compact" className="mt-3">
        <div className="flex items-center gap-3.5">
          <QrCode text={status.address} label={`QR code for ${host}`} />
          <div className="min-w-0">
            <div className={K}>Cloud address</div>
            <div className="mb-2.5 mt-0.5 truncate font-mono text-[13px]">{host}</div>
            <a href={status.address} target="_blank" rel="noreferrer" className={buttonClassName("secondary", "", "sm")}>
              Open ↗
            </a>
            <p className="mt-2.5 text-[11.5px] text-mut">
              Scan to open the webapp on your phone. You sign in there and type your PIN.
            </p>
          </div>
        </div>
      </Card>
      <Disconnect />
    </section>
  );
}

function StatusError({ error, onRetry, retrying }: { error: Error; onRetry: () => void; retrying: boolean }) {
  const disconnect = useCloudMutation(cloudApi.disconnect);
  // An unreadable keys.json entry answers 500, and Disconnect is the only way out of it.
  const unreadable = error instanceof CloudError && error.code === "internal";
  return (
    <Note tone="straw" role="alert" className="mb-3">
      <div>Could not load the Cloud sync status: {error.message}</div>
      <div className="mt-2 flex gap-2">
        <Button variant="secondary" size="sm" onClick={onRetry} disabled={retrying}>
          Retry
        </Button>
        {unreadable && (
          <Button variant="secondary" size="sm" disabled={disconnect.isPending} onClick={() => disconnect.mutate(false)}>
            Disconnect
          </Button>
        )}
      </div>
    </Note>
  );
}

export function CloudSection({ view }: { view?: string }) {
  const q = useCloudStatus();
  const data = q.data;
  // A failed refetch keeps the last good data, so the section stays usable with the error shown above it.
  const failure = q.isError ? (
    <StatusError error={q.error} onRetry={() => void q.refetch()} retrying={q.isFetching} />
  ) : null;

  if (!data) {
    return failure ?? (
      <div className="max-w-md space-y-3" aria-busy="true" data-testid="cloud-loading">
        <div className="h-7 w-40 rounded shimmer" />
        <div className="h-5 w-3/4 rounded shimmer" />
        <div className="h-24 w-full rounded-card shimmer" />
      </div>
    );
  }
  if (data.connected) {
    return view === "connect" ? (
      <Navigate to="/settings/cloud" replace />
    ) : (
      <>
        {failure}
        <Connected status={data} />
      </>
    );
  }
  return (
    <>
      {failure}
      {view === "connect" ? <ConnectForm /> : <NotSetUp />}
    </>
  );
}
