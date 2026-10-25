import { useEffect, useState } from "react";
import { Button } from "@web/components/Button";
import { AuthShell, Checkbox, FieldLabel, Keypad, LockIcon, Note, PinField, Segmented, TextLink, h3, sub } from "../ui";
import { formatWait, lockoutMinutes } from "../lib/route";

type Len = "4" | "6";
export const LENGTHS = [
  { value: "4" as Len, label: "4 digits" },
  { value: "6" as Len, label: "6 digits" },
];

/** New PIN plus confirmation. Shared by first-time setup and Change PIN. */
export function NewPin({ onSubmit, submit, busy, error, autoFocus = true }: { onSubmit: (pin: string) => void; submit: string; busy: boolean; error: string | null; autoFocus?: boolean }) {
  const [len, setLen] = useState<Len>("6");
  const [pin, setPin] = useState("");
  const [confirm, setConfirm] = useState("");
  const n = Number(len) as 4 | 6;
  const mismatch = confirm.length === n && confirm !== pin;
  return (
    <form
      className="contents"
      onSubmit={(e) => {
        e.preventDefault();
        if (pin.length === n && pin === confirm) onSubmit(pin);
      }}
    >
      <Segmented label="PIN length" options={LENGTHS} value={len} onChange={(v) => { setLen(v); setPin(""); setConfirm(""); }} />
      <div className="flex flex-col gap-[5px]">
        <FieldLabel>PIN</FieldLabel>
        <PinField label="PIN" length={n} value={pin} onChange={setPin} autoFocus={autoFocus} onFull={() => document.getElementById("confirm-pin")?.focus()} />
      </div>
      <div className="flex flex-col gap-[5px]">
        <FieldLabel>Confirm PIN</FieldLabel>
        <PinField id="confirm-pin" label="Confirm PIN" length={n} value={confirm} onChange={setConfirm} error={mismatch} />
      </div>
      {mismatch && <Note>The two PINs differ.</Note>}
      {error && <Note>{error}</Note>}
      <Button type="submit" disabled={busy || pin.length !== n || pin !== confirm} className="w-full !leading-normal">
        {submit}
      </Button>
    </form>
  );
}

export function ChoosePin({ onSubmit, busy, error }: { onSubmit: (pin: string) => void; busy: boolean; error: string | null }) {
  return (
    <AuthShell>
      <LockIcon />
      <h1 className={h3}>Choose a 4 or 6 digit PIN</h1>
      <p className={sub}>You type it in a browser to read meetings. Laptops never see it.</p>
      <NewPin submit="Save PIN" onSubmit={onSubmit} busy={busy} error={error} />
    </AuthShell>
  );
}

export function Unlock({ email, lockedUntil, notice, onUnlock, onSignOut }: { email: string; lockedUntil: string | null; notice: string | null; onUnlock: (pin: string, remember: boolean) => Promise<"ok" | "wrong">; onSignOut: () => void }) {
  const [pin, setPin] = useState("");
  const [remember, setRemember] = useState(true);
  const [wrong, setWrong] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!lockedUntil) return;
    const t = setInterval(() => setNow(Date.now()), 15_000);
    return () => clearInterval(t);
  }, [lockedUntil]);
  const minutes = lockoutMinutes(lockedUntil, now);
  const locked = minutes > 0;
  const valid = pin.length === 4 || pin.length === 6;

  const submit = async () => {
    if (!valid || busy || locked) return;
    setBusy(true);
    setFailed(null);
    try {
      if ((await onUnlock(pin, remember)) === "wrong") {
        setWrong(true);
        setPin("");
      }
    } catch (e) {
      setFailed(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <AuthShell top>
      <h1 className={h3}>Unlock your meetings</h1>
      <p className={sub}>
        Signed in as <span className="min-[721px]:text-ink min-[721px]:font-bold">{email}</span>
      </p>
      <form
        className="contents"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <PinField label="PIN" length={6} value={pin} onChange={(v) => { setWrong(false); setPin(v); }} error={wrong || locked} disabled={locked} keypad autoFocus />
        {locked && <Note>Too many wrong tries. Try again in {formatWait(minutes)}.</Note>}
        {wrong && !locked && <Note>Wrong PIN. Try again.</Note>}
        {(failed ?? notice) && <Note>{failed ?? notice}</Note>}
        <div>
          <Checkbox checked={remember} onChange={setRemember}>
            Remember this browser
          </Checkbox>
          <p className="hidden min-[721px]:block text-[11.5px] text-mut mt-[4px] mb-[0px]">Checked: unlocked for 90 days. Unchecked: 12 hours.</p>
        </div>
        <Keypad disabled={locked} onDigit={(d) => { setWrong(false); setPin((p) => (p.length < 6 ? p + d : p)); }} onBackspace={() => setPin((p) => p.slice(0, -1))} />
        <Button type="submit" disabled={!valid || busy || locked} className="w-full !leading-normal">
          Unlock
        </Button>
      </form>
      <TextLink lead="Not you?" onClick={onSignOut}>
        Sign out
      </TextLink>
    </AuthShell>
  );
}
