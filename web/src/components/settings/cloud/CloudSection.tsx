import { useEffect, useState } from "react";
import { Navigate, useNavigate } from "react-router";
import { Button, buttonClassName } from "../../Button";
import { Card } from "../../Card";
import { Pill } from "../../Pill";
import {
  CloudError,
  cloudApi,
  useCloudMutation,
  useCloudStatus,
  type CloudConnected,
  type LastPush,
} from "../../../lib/api/cloud";
import { ConnectForm } from "./ConnectForm";
import { INPUT, K } from "./Field";
import { Note } from "./Note";
import { QrCode } from "./QrCode";

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

export function describeLastPush(p: LastPush | null): string {
  if (!p) return "Not pushed yet";
  return `Last push ${formatPushTime(p.at)} · ${p.changed} uploaded, ${p.deleted} deleted`;
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

function Stat({ value, label, tone }: { value: number; label: string; tone?: string }) {
  return (
    <div className="flex flex-col">
      <b className={`text-[18px] font-extrabold ${tone ?? ""}`}>{value}</b>
      <span className="text-[11px] text-mut">{label}</span>
    </div>
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
  const error = needsSignIn ? null : (push.error?.message ?? status.last_error?.error);
  return (
    <section>
      <Header connected intro={`Signed in as ${status.email}`} />
      <Card active padding="compact" className="border-[1px]! ring-1 ring-blue">
        <div className="flex flex-wrap items-center justify-between gap-2.5">
          <div>
            <div className={K}>Machine name</div>
            <b className="text-[15px]">{status.machine_name}</b>
          </div>
          <Button size="sm" disabled={push.isPending || needsSignIn} onClick={() => push.mutate(undefined)}>
            {push.isPending ? "Pushing…" : "Push meetings"}
          </Button>
        </div>
        <div className="mb-3.5 mt-2.5 flex flex-wrap gap-6">
          <Stat value={status.meetings} label="meetings on this Mac" />
          <Stat value={status.not_pushed} label="not pushed" tone={status.not_pushed > 0 ? "text-honeyink" : ""} />
        </div>
        <div className="text-[11.5px] text-mut">{describeLastPush(status.last_push)}</div>
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
