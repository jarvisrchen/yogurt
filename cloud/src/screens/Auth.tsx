import { useState } from "react";
import { Button } from "@web/components/Button";
import { AuthShell, Field, MailIcon, Note, TextLink, h3, sub } from "../ui";
import type { Api } from "../lib/api";

type Mode = "signin" | "create" | "check-email";

function Credentials({ onSubmit, submit, busy, error, defaultEmail }: { onSubmit: (email: string, password: string) => void; submit: string; busy: boolean; error: string | null; defaultEmail: string }) {
  const [email, setEmail] = useState(defaultEmail);
  const [password, setPassword] = useState("");
  return (
    <form
      className="contents"
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit(email.trim(), password);
      }}
    >
      <Field label="Email" type="email" autoComplete="username" required value={email} onChange={(e) => setEmail(e.target.value)} placeholder="you@example.com" />
      <Field label="Password" type="password" autoComplete={submit === "Sign in" ? "current-password" : "new-password"} required minLength={submit === "Sign in" ? undefined : 6} value={password} onChange={(e) => setPassword(e.target.value)} placeholder="Password" />
      {error && <Note>{error}</Note>}
      <Button type="submit" disabled={busy} className="w-full !leading-normal">
        {submit}
      </Button>
    </form>
  );
}

/** Sign in, create the Cloud login, and the check-your-email screen. A session arriving ends all three through onAuthStateChange. */
export function AuthFlow({ api }: { api: Api }) {
  const [mode, setMode] = useState<Mode>("signin");
  const [email, setEmail] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (em: string, fn: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    setEmail(em);
    try {
      await fn();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  if (mode === "check-email") {
    return (
      <AuthShell>
        <MailIcon />
        <h1 className={h3}>Check your email</h1>
        <p className={sub}>
          We sent a confirmation link to {email}. Click it, then come back and sign in.
        </p>
        <Button variant="secondary" className="w-full !leading-normal" onClick={() => { setError(null); setMode("signin"); }}>
          Sign in
        </Button>
      </AuthShell>
    );
  }

  if (mode === "create") {
    return (
      <AuthShell>
        <h1 className={h3}>Create your Cloud login</h1>
        <p className={sub}>
          Use the email you entered as <span className="font-mono text-[13px] min-[721px]:text-[10.5px]">YOGURT_OWNER_EMAIL</span> when you deployed.
        </p>
        <Credentials
          submit="Create login"
          busy={busy}
          error={error}
          defaultEmail={email}
          onSubmit={(em, pw) => run(em, async () => { if (!(await api.signUp(em, pw))) setMode("check-email"); })}
        />
        <TextLink lead="Already have a login?" onClick={() => { setError(null); setMode("signin"); }}>
          Sign in
        </TextLink>
      </AuthShell>
    );
  }

  return (
    <AuthShell>
      <h1 className={h3}>Sign in</h1>
      <p className={`${sub} min-[721px]:hidden`}>Once per browser. After this you only type your PIN.</p>
      <Credentials submit="Sign in" busy={busy} error={error} defaultEmail={email} onSubmit={(em, pw) => run(em, () => api.signIn(em, pw))} />
      <TextLink lead="First time here?" onClick={() => { setError(null); setMode("create"); }}>
        Create your Cloud login
      </TextLink>
    </AuthShell>
  );
}

export function NotOwner({ email, onSignOut }: { email: string; onSignOut: () => void }) {
  return (
    <AuthShell>
      <h1 className={h3}>This login does not own this Yogurt Cloud</h1>
      <p className={sub}>
        {email} is signed in, but the owner is the email entered as <span className="font-mono text-[13px] min-[721px]:text-[10.5px]">YOGURT_OWNER_EMAIL</span> when this Yogurt Cloud was deployed.
      </p>
      <Button variant="secondary" className="w-full !leading-normal" onClick={onSignOut}>
        Sign out
      </Button>
    </AuthShell>
  );
}
