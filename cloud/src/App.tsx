import { useCallback, useEffect, useState } from "react";
import { Api, loadConfig, TokenRejected } from "./lib/api";
import { HOME, useLoc } from "./lib/hash";
import { browserLabel, formatWait, lockoutMinutes, screenFor, type Status } from "./lib/route";
import { clearToken, readToken, saveToken, type StoredToken } from "./lib/token";
import { AuthFlow, NotOwner } from "./screens/Auth";
import { History } from "./screens/History";
import { Library } from "./screens/Library";
import { ChoosePin, Unlock } from "./screens/Pin";
import { Settings } from "./screens/Settings";
import { AuthShell, Note, h3, sub } from "./ui";

export function App() {
  const [api, setApi] = useState<Api | null>(null);
  const [fatal, setFatal] = useState<string | null>(null);
  useEffect(() => {
    loadConfig().then((c) => setApi(new Api(c)), (e) => setFatal(String(e.message ?? e)));
  }, []);
  if (fatal)
    return (
      <AuthShell>
        <h1 className={h3}>This Yogurt Cloud is not set up</h1>
        <p className={sub}>{fatal}</p>
      </AuthShell>
    );
  return api ? <Signed api={api} /> : null;
}

function Signed({ api }: { api: Api }) {
  const [email, setEmail] = useState<string | null | undefined>(undefined);
  const [status, setStatus] = useState<Status | null>(null);
  const [stored, setStored] = useState<StoredToken | null>(readToken);
  const { loc, go, back, reset } = useLoc();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pinChanged, setPinChanged] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    // INITIAL_SESSION always fires, so this one subscription is the only source of the session.
    const { data } = api.sb.auth.onAuthStateChange((_e, s) => setEmail(s?.user.email ?? null));
    return () => data.subscription.unsubscribe();
  }, [api]);

  const refreshStatus = useCallback(() => api.status().then(setStatus, (e) => setError(e.message)), [api]);
  useEffect(() => {
    setStatus(null);
    if (email) void refreshStatus();
  }, [email, refreshStatus]);

  useEffect(() => {
    if (loc.view !== "settings" && loc.view !== "change-pin") setPinChanged(false);
  }, [loc.view]);

  const token = stored?.token ?? null;
  const forget = useCallback(() => {
    clearToken();
    setStored(null);
  }, []);
  // A rejected token also means the lockout state may have changed, so reread the status. The location stays, so unlocking lands on it.
  const drop = useCallback(() => {
    forget();
    void refreshStatus();
  }, [forget, refreshStatus]);
  // An explicit Lock or Sign out starts over at the library.
  const lockOut = useCallback(() => {
    drop();
    reset();
  }, [drop, reset]);
  const keep = (t: StoredToken) => {
    saveToken(t);
    setStored(t);
  };

  const unlock = async (pin: string, remember: boolean) => {
    const t = await api.unlock(pin, remember, browserLabel());
    if (!t) {
      await refreshStatus();
      return "wrong" as const;
    }
    keep({ token: t, remember });
    setNotice(null);
    return "ok" as const;
  };

  const lock = async () => {
    if (token) await api.lock(token).catch(() => undefined);
    lockOut();
  };
  const signOut = async () => {
    if (token) await api.lock(token).catch(() => undefined);
    forget();
    reset();
    setNotice(null);
    await api.signOut();
  };

  const choosePin = async (pin: string) => {
    setBusy(true);
    setError(null);
    try {
      if (!(await api.setPasscode(pin))) throw new Error("The PIN was not saved.");
      await unlock(pin, true);
      await refreshStatus();
    } catch (e) {
      // The save may have landed with the reply lost, which makes a retry fail. Routing off the fresh status lands on Unlock.
      const st = await api.status().catch(() => null);
      if (st) setStatus(st);
      if (!st?.passcode_set) setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const changePin = async (oldPin: string, newPin: string): Promise<string | null> => {
    try {
      if (!(await api.setPasscode(newPin, oldPin))) {
        const st = await api.status();
        setStatus(st);
        const wait = lockoutMinutes(st.locked_until);
        return wait ? `Too many wrong tries. Try again in ${formatWait(wait)}.` : "The current PIN is wrong.";
      }
    } catch (e) {
      if (e instanceof TokenRejected) drop();
      return e instanceof Error ? e.message : String(e);
    }
    // Every browser was forgotten, this one included, so unlock it again the way it was unlocked.
    const remember = stored?.remember ?? false;
    try {
      const t = await api.unlock(newPin, remember, browserLabel());
      if (t) {
        keep({ token: t, remember });
        setStatus((st) => st && { ...st, passcode_length: newPin.length });
        setPinChanged(true);
        back({ ...HOME, view: "settings" });
        return null;
      }
    } catch {
      // Falls through to the Unlock screen.
    }
    setNotice("The PIN was changed, but this browser could not unlock again. Type the new PIN.");
    lockOut();
    return null;
  };

  const screen = screenFor({ signedIn: !!email, status, token, view: loc.view });

  if (email === undefined) return null;
  if (error && screen === "loading")
    return (
      <AuthShell>
        <Note>{error}</Note>
      </AuthShell>
    );
  switch (screen) {
    case "loading":
      return null;
    case "signin":
      return <AuthFlow api={api} />;
    case "not-owner":
      return <NotOwner email={email!} onSignOut={signOut} />;
    case "choose-pin":
      return <ChoosePin onSubmit={choosePin} busy={busy} error={error} />;
    case "unlock":
      return <Unlock email={email!} length={status!.passcode_length} lockedUntil={status!.locked_until} notice={notice} onUnlock={unlock} onSignOut={signOut} />;
    case "library":
      return (
        <Library
          api={api}
          token={token!}
          loc={loc}
          go={go}
          back={back}
          onSettings={() => go({ ...HOME, view: "settings" })}
          onLock={lock}
          onRejected={drop}
        />
      );
    case "history":
      return (
        <History
          api={api}
          token={token!}
          onBack={() => back({ ...HOME, view: "settings" })}
          onOpen={(r) => go({ ...HOME, asOf: r.push_id })}
          onLock={lock}
          onRejected={drop}
        />
      );
    case "settings":
    case "change-pin":
      return (
        <Settings
          api={api}
          token={token!}
          email={email!}
          remember={stored!.remember}
          view={screen}
          setView={(v) => (v === "settings" ? back({ ...HOME, view: "settings" }) : go({ ...HOME, view: v }))}
          changed={pinChanged}
          onBack={() => back(HOME)}
          onHistory={() => go({ ...HOME, view: "history" })}
          onLock={lock}
          onSignOut={signOut}
          onRejected={drop}
          pinLength={status!.passcode_length}
          onChangePin={changePin}
        />
      );
  }
}
