import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Button } from "@web/components/Button";
import { getThemePref, setThemePref, type ThemePref } from "@web/lib/theme";
import { Back, Brand, LockButton, FieldLabel, LoadError, PinField, Segmented, useDesktop, h3 } from "../ui";
import { NewPin } from "./Pin";
import { pinBoxes, validPin } from "../lib/route";
import { TokenRejected, type Api, type Browser } from "../lib/api";

const THEMES: { value: ThemePref; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

const date = (iso: string) => new Date(iso).toLocaleDateString("en-US", { month: "short", day: "numeric" });
const dayLabel = "font-mono text-[11px] min-[721px]:text-[10.5px] uppercase tracking-[.08em] text-mut mt-[4px] min-[721px]:mt-[10px]";

const Group = ({ title, children }: { title: string; children: ReactNode }) => (
  <>
    <div className={dayLabel}>{title}</div>
    {children}
  </>
);
const GroupBox = ({ children }: { children: ReactNode }) => (
  <div className="bg-card border border-line rounded-button overflow-hidden [&>*:not(:last-child)]:border-b [&>*]:border-line">{children}</div>
);
const Row = ({ children, onClick }: { children: ReactNode; onClick?: () => void }) => {
  const cls = "flex w-full items-center justify-between gap-[8px] px-[11px] py-[9px] min-[721px]:px-[14px] min-[721px]:py-[11px] text-[14px] min-[721px]:text-[13px] text-left font-[inherit] text-ink bg-transparent border-0";
  return onClick ? (
    <button type="button" onClick={onClick} className={`${cls} cursor-pointer`}>
      {children}
    </button>
  ) : (
    <div className={cls}>{children}</div>
  );
};
const Small = ({ children, mono }: { children: ReactNode; mono?: boolean }) => (
  <small className={`block text-[12px] min-[721px]:text-[11px] text-mut mt-px ${mono ? "font-mono" : ""}`}>{children}</small>
);
const Chev = () => <span className="text-mut flex-none">›</span>;
const ForgetPill = ({ onClick, label }: { onClick: () => void; label: string }) => (
  <button type="button" aria-label={label} onClick={onClick} className="text-[12px] min-[721px]:text-[10px] leading-normal font-bold px-[10px] py-[3px] rounded-pill bg-strsoft text-straw border-0 cursor-pointer font-[inherit]">
    Forget
  </button>
);

interface Props {
  api: Api;
  token: string;
  email: string;
  remember: boolean;
  onBack: () => void;
  onHistory: () => void;
  onLock: () => void;
  onSignOut: () => void;
  onRejected: () => void;
  pinLength: number | null;
  /** Sets the new PIN, then unlocks this browser again with it. Resolves to an error message, or null on success. */
  onChangePin: (oldPin: string, newPin: string) => Promise<string | null>;
  view: "settings" | "change-pin";
  setView: (v: "settings" | "change-pin") => void;
  changed: boolean;
}

export function Settings(p: Props) {
  const desktop = useDesktop();
  const [browsers, setBrowsers] = useState<Browser[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [theme, setTheme] = useState(getThemePref);
  const { api, token, onRejected } = p;
  const load = useCallback(() => {
    setError(null);
    api.browsers(token).then(setBrowsers, (e) => (e instanceof TokenRejected ? onRejected() : setError(e.message)));
  }, [api, token, onRejected]);
  useEffect(load, [load, p.changed]);

  const forget = async (b: Browser) => {
    if (b.current) return p.onLock();
    try {
      await api.forgetBrowser(token, b.id);
      load();
    } catch (e) {
      if (e instanceof TokenRejected) onRejected();
      else setError(e instanceof Error ? e.message : String(e));
    }
  };

  if (p.view === "change-pin") return <ChangePin {...p} />;

  const host = location.host;
  const bar = desktop && (
    <div className="flex items-center justify-between gap-[10px] px-[20px] py-[12px] bg-card border-b border-line">
      <Back to="Library" onClick={p.onBack} />
      <LockButton onClick={p.onLock} />
    </div>
  );

  return (
    <div className="min-h-dvh flex flex-col">
      {bar}
      <main className="w-full max-w-[640px] mx-auto flex flex-col gap-[10px] px-[16px] pt-[16px] pb-[16px] min-[721px]:px-[24px] min-[721px]:pt-[22px] min-[721px]:pb-[28px]">
        {!desktop && <Back to="Library" onClick={p.onBack} />}
        <h1 className="text-[22px] font-extrabold tracking-[-0.01em] text-ink m-[0px] min-[721px]:mb-[2px]">Settings</h1>
        {p.changed && <div className="text-[15px] min-[721px]:text-[12px] rounded-button px-[12px] py-[9px] bg-mtsoft text-matcha" role="status">PIN changed. This browser is unlocked with the new PIN.</div>}
        <Group title="Account">
          <GroupBox>
            <Row>
              <div>
                {p.email}
                <Small mono>{host}</Small>
              </div>
              {desktop && <Button variant="secondary" onClick={p.onSignOut} className="!px-[14px] !py-[8px] !text-[12.5px] !font-bold !leading-normal">Sign out</Button>}
            </Row>
            {!desktop && (
              <Row onClick={p.onSignOut}>
                <div>Sign out</div>
                <Chev />
              </Row>
            )}
          </GroupBox>
        </Group>
        <Group title="PIN">
          <GroupBox>
            <Row onClick={desktop ? undefined : p.onLock}>
              <div>
                Lock
                <Small>Forget the token in this browser</Small>
              </div>
              {desktop ? <Button variant="secondary" onClick={p.onLock} className="!px-[14px] !py-[8px] !text-[12.5px] !font-bold !leading-normal">Lock</Button> : <Chev />}
            </Row>
            <Row onClick={desktop ? undefined : () => p.setView("change-pin")}>
              <div>
                Change PIN
                {desktop && <Small>4 or 6 digits. Forgets every other remembered browser.</Small>}
              </div>
              {desktop ? <Button variant="secondary" onClick={() => p.setView("change-pin")} className="!px-[14px] !py-[8px] !text-[12.5px] !font-bold !leading-normal">Change PIN</Button> : <Chev />}
            </Row>
          </GroupBox>
        </Group>
        <Group title="Remembered browsers">
          {error ? (
            <LoadError message={`Could not load remembered browsers. ${error}`} onRetry={load} />
          ) : desktop ? (
            <div className="bg-card border border-line rounded-card overflow-hidden">
              <table className="w-full border-separate border-spacing-0 text-[12.5px]">
                <thead>
                  <tr className="bg-paper">
                    {["Browser", "Unlocked", "Expires", ""].map((h, i) => (
                      <th key={i} className="text-left font-mono text-[10px] uppercase tracking-[.08em] text-mut font-medium px-[14px] py-[10px] border-b border-line">
                        {h}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {(browsers ?? []).map((b, i, a) => (
                    <tr key={b.id}>
                      {[
                        <>
                          <b>{b.label || "Unnamed browser"}</b>
                          {b.current && <span className="ml-[4px] text-[10px] leading-normal font-bold px-[10px] py-[3px] rounded-pill bg-blsoft text-blue">This browser</span>}
                        </>,
                        date(b.created_at),
                        date(b.expires_at),
                        <ForgetPill label={`Forget ${b.label}`} onClick={() => forget(b)} />,
                      ].map((c, j) => (
                        <td key={j} className={`px-[14px] py-[10px] ${i < a.length - 1 ? "border-b border-line" : ""}`}>
                          {c}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <GroupBox>
              {(browsers ?? []).map((b) => (
                <Row key={b.id}>
                  <div>
                    {b.label || "Unnamed browser"}
                    <Small>
                      {b.current ? "This browser · " : ""}expires {date(b.expires_at)}
                    </Small>
                  </div>
                  <ForgetPill label={`Forget ${b.label}`} onClick={() => forget(b)} />
                </Row>
              ))}
            </GroupBox>
          )}
        </Group>
        <Group title="Appearance">
          <GroupBox>
            <Row>
              <div>Theme</div>
              <Segmented
                label="Appearance"
                options={THEMES}
                value={theme}
                onChange={(v) => {
                  setThemePref(v);
                  setTheme(v);
                }}
              />
            </Row>
          </GroupBox>
        </Group>
        <Group title="Recovery">
          <GroupBox>
            <Row onClick={p.onHistory}>
              <div>
                History
                <Small>See your library as of an earlier push</Small>
              </div>
              <Chev />
            </Row>
          </GroupBox>
        </Group>
      </main>
    </div>
  );
}

function ChangePin(p: Props) {
  const [old, setOld] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return (
    <div className="min-h-dvh flex flex-col">
      <main className="w-full max-w-[380px] mx-auto flex flex-col gap-[14px] px-[16px] pt-[16px] pb-[16px] min-[721px]:pt-[40px]">
        <Back to="Settings" onClick={() => p.setView("settings")} />
        <Brand />
        <h1 className={h3}>Change PIN</h1>
        <div className="flex flex-col gap-[5px]">
          <FieldLabel>Current PIN</FieldLabel>
          <PinField label="Current PIN" length={pinBoxes(p.pinLength, old)} max={6} value={old} onChange={setOld} autoFocus />
          <p className="text-[14px] min-[721px]:text-[11.5px] text-mut m-[0px]">{p.pinLength ? `Type the ${p.pinLength} digits you use today.` : "Type the 4 digits or 6 digits you use today."}</p>
        </div>
        <NewPin
          submit="Save PIN"
          autoFocus={false}
          busy={busy}
          error={error}
          onSubmit={async (pin) => {
            if (!validPin(old)) return setError("Enter your current PIN first.");
            setBusy(true);
            setError(null);
            try {
              setError(await p.onChangePin(old, pin));
            } catch (e) {
              setError(e instanceof Error ? e.message : String(e));
            } finally {
              setBusy(false);
            }
          }}
        />
      </main>
    </div>
  );
}
