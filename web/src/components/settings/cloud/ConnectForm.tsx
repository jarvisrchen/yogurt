import { useState } from "react";
import { useNavigate } from "react-router";
import { Button } from "../../Button";
import { Card } from "../../Card";
import { cloudApi, useCloudMutation } from "../../../lib/api/cloud";
import { Field, INPUT, INPUT_MONO } from "./Field";
import { Note } from "./Note";

/** The result belongs to the address it checked, so editing the address hides it without any effect to reset it. */
type Found = { address: string; state: "checking" | "ok" | { error: string } } | null;

export function ConnectForm() {
  const navigate = useNavigate();
  const connect = useCloudMutation(cloudApi.connect);
  const [address, setAddress] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [name, setName] = useState("");
  const [found, setFound] = useState<Found>(null);
  const [error, setError] = useState<string | null>(null);

  const trimmed = address.trim();
  const shown = found?.address === trimmed ? found.state : null;

  const checkAddress = async () => {
    if (!trimmed || found?.address === trimmed) return;
    setFound({ address: trimmed, state: "checking" });
    try {
      await cloudApi.check(trimmed);
      setFound({ address: trimmed, state: "ok" });
    } catch (e) {
      setFound({ address: trimmed, state: { error: e instanceof Error ? e.message : String(e) } });
    }
  };

  const ready = trimmed && email.trim() && password && name.trim();
  const submit = async () => {
    setError(null);
    try {
      await connect.mutateAsync({
        address: trimmed,
        email: email.trim(),
        password,
        machine_name: name.trim(),
      });
      navigate("/settings/cloud");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      // The password only ever lives in this field, so a failed attempt clears it.
      setPassword("");
    } finally {
      connect.reset();
    }
  };

  const bad = shown !== null && typeof shown === "object";
  return (
    <section>
      <header>
        <h2 className="heading-section">Connect to a Yogurt Cloud</h2>
        <p className="mb-4 mt-1 text-[13px] text-ai">
          Use the address of the webapp you deployed and its Cloud login.
        </p>
      </header>
      <Card padding="compact">
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (ready && !connect.isPending) void submit();
          }}
        >
          <Field
            label="Cloud address"
            htmlFor="cloud-address"
            hint={
              shown === "ok" ? (
                <span className="text-matcha">✓ Found a Yogurt Cloud</span>
              ) : bad ? (
                <span role="alert" className="text-straw">
                  {shown.error}
                </span>
              ) : shown === "checking" ? (
                "Checking…"
              ) : undefined
            }
          >
            <input
              id="cloud-address"
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              onBlur={() => void checkAddress()}
              placeholder="your-yogurt.vercel.app"
              autoComplete="off"
              autoCapitalize="off"
              spellCheck={false}
              className={`${INPUT_MONO} ${shown === "ok" ? "border-matcha" : bad ? "border-straw" : "border-line"}`}
            />
          </Field>
          <div className="grid grid-cols-[repeat(auto-fit,minmax(180px,1fr))] gap-x-3">
            <Field label="Login email" htmlFor="cloud-email">
              <input
                id="cloud-email"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                autoComplete="off"
                className={`${INPUT} border-line`}
              />
            </Field>
            <Field label="Login password" htmlFor="cloud-password">
              <input
                id="cloud-password"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="off"
                className={`${INPUT} border-line`}
              />
            </Field>
          </div>
          <Field label="Machine name" htmlFor="cloud-machine" hint="Shown in the device picker in the webapp.">
            <input
              id="cloud-machine"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Laptop 2"
              maxLength={80}
              autoComplete="off"
              className={`${INPUT} border-line`}
            />
          </Field>
          <Note tone="matcha">
            This laptop can push its own meetings and list its own meeting ids. It never reads content, and it never
            sees the PIN.
          </Note>
          {error && (
            <Note tone="straw" role="alert" className="mt-3">
              {error}
            </Note>
          )}
          <div className="mt-3.5 flex items-center justify-end gap-2.5">
            <Button type="button" variant="ghost" size="sm" className="border-line!" onClick={() => navigate("/settings/cloud")}>
              Cancel
            </Button>
            <Button type="submit" size="sm" disabled={!ready || connect.isPending}>
              {connect.isPending ? "Connecting…" : "Connect"}
            </Button>
          </div>
        </form>
      </Card>
    </section>
  );
}
