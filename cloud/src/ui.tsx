import { useEffect, useId, useRef, useState, useSyncExternalStore, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { Button } from "@web/components/Button";
import { DEFAULT_CUSTOM_COLOR, LABEL_COLORS, isHexColor, labelTone } from "@web/lib/labelColors";
import { Logo } from "@web/components/Logo";
import { PAGER_BUTTON } from "@web/components/library/Pager";
import { TokenRejected, type Api, type Device, type Label } from "./lib/api";
import { snippetParts } from "./lib/search";

const DESKTOP = "(min-width: 721px)";

function subscribeDesktop(cb: () => void) {
  const mq = matchMedia(DESKTOP);
  mq.addEventListener("change", cb);
  return () => mq.removeEventListener("change", cb);
}
const isDesktop = () => matchMedia(DESKTOP).matches;

export function useDesktop(): boolean {
  return useSyncExternalStore(subscribeDesktop, isDesktop);
}

export const h3 = "text-[22px] min-[721px]:text-[20px] font-extrabold tracking-[-0.01em] text-ink m-[0px]";
export const sub = "text-[15px] min-[721px]:text-[12.5px] text-mut m-[0px]";
const kLabel = "font-mono text-[12px] min-[721px]:text-[10px] uppercase tracking-[.08em] text-mut";

export function Brand() {
  return (
    <div className="flex items-center gap-[8px]">
      <Logo size={28} />
      <span className="heading-wordmark">yogurt</span>
    </div>
  );
}

/** Auth screens: logo over a vertically centered column on a phone, a centered card on a desktop. */
export function AuthShell({ children, top }: { children: ReactNode; top?: boolean }) {
  return (
    <main className="min-h-dvh flex flex-col px-[16px] pt-[16px] pb-[16px] min-[721px]:items-center min-[721px]:justify-center min-[721px]:p-[40px]">
      <div className="flex-1 flex flex-col gap-[10px] w-full min-[721px]:flex-none min-[721px]:max-w-[380px] min-[721px]:gap-[14px] min-[721px]:bg-card min-[721px]:border min-[721px]:border-line min-[721px]:rounded-card min-[721px]:shadow-card min-[721px]:p-[28px]">
        <Brand />
        <div className={`flex-1 flex flex-col gap-[12px] min-[721px]:flex-none min-[721px]:gap-[14px] min-[721px]:justify-start ${top ? "justify-start pt-[2px] gap-[9px]" : "justify-center"}`}>{children}</div>
      </div>
    </main>
  );
}

export function Field({ label, ...rest }: { label: string } & React.InputHTMLAttributes<HTMLInputElement>) {
  return (
    <label className="flex flex-col gap-[5px]">
      <span className={kLabel}>{label}</span>
      <input
        {...rest}
        className="font-sans text-[16px] min-[721px]:text-[13px] bg-paper text-ink placeholder:text-grey border border-line rounded-button px-[12px] py-[10px] outline-none focus:border-blue focus:ring-1 focus:ring-blue"
      />
    </label>
  );
}

export function FieldLabel({ children }: { children: ReactNode }) {
  return <span className={kLabel}>{children}</span>;
}

function IconTile({ tone = "blue", children }: { tone?: "blue" | "matcha"; children: ReactNode }) {
  return (
    <div className={`w-[44px] h-[44px] rounded-[14px] grid place-items-center ${tone === "blue" ? "bg-blsoft text-blue" : "bg-mtsoft text-matcha"}`}>
      <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden>
        {children}
      </svg>
    </div>
  );
}

export const LockIcon = () => (
  <IconTile>
    <rect x="5" y="10.5" width="14" height="10" rx="2.5" />
    <path d="M8 10.5V8a4 4 0 018 0v2.5" />
  </IconTile>
);

export const MailIcon = () => (
  <IconTile tone="matcha">
    <rect x="3" y="5.5" width="18" height="13" rx="2.5" />
    <path d="M3.5 7.5l8.5 6 8.5-6" />
  </IconTile>
);

export function Note({ children }: { children: ReactNode }) {
  return (
    <div role="alert" className="flex gap-[8px] items-start text-[15px] min-[721px]:text-[12px] rounded-button px-[12px] py-[9px] bg-strsoft text-straw">
      <svg viewBox="0 0 16 16" width="15" height="15" className="flex-none mt-px" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden>
        <path d="M8 2l6.5 11.5h-13z" />
        <path d="M8 6.5v3.2M8 11.6v.1" />
      </svg>
      <div>{children}</div>
    </div>
  );
}

/** A failed read, with a way to try again. Without it a network error would look like an empty library. */
export function LoadError({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div className="flex flex-col gap-[10px] items-start">
      <Note>{message}</Note>
      <Button variant="secondary" onClick={onRetry} className="!px-[14px] !py-[8px] !text-[15px] min-[721px]:!text-[12.5px] !font-bold !leading-normal">
        Retry
      </Button>
    </div>
  );
}

export function TextLink({ lead, children, onClick }: { lead?: string; children: ReactNode; onClick: () => void }) {
  return (
    <div className="text-[14px] min-[721px]:text-[12.5px] font-semibold text-center text-mut">
      {lead}
      {lead ? " " : ""}
      <button type="button" onClick={onClick} className="text-blue font-semibold cursor-pointer bg-transparent border-0 p-[0px] font-[inherit]">
        {children}
      </button>
    </div>
  );
}

export function Segmented<T extends string>({ options, value, onChange, label }: { options: { value: T; label: string }[]; value: T; onChange: (v: T) => void; label: string }) {
  return (
    <div role="group" aria-label={label} className="inline-flex self-start bg-paper border border-line rounded-button p-[3px] gap-[2px]">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          aria-pressed={o.value === value}
          onClick={() => onChange(o.value)}
          className={`font-sans text-[13px] min-[721px]:text-[11px] font-semibold px-[12px] py-[3px] rounded-[9px] cursor-pointer border-0 ${o.value === value ? "bg-card text-ink shadow-[0_2px_6px_rgba(40,30,15,.10)]" : "bg-transparent text-mut"}`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Checkbox({ checked, onChange, children }: { checked: boolean; onChange: (v: boolean) => void; children: ReactNode }) {
  return (
    <label className="flex items-center gap-[8px] text-[14px] min-[721px]:text-[13px] cursor-pointer">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} className="sr-only peer" />
      <span className={`w-[14px] h-[14px] rounded-[4px] grid place-items-center flex-none border peer-focus-visible:ring-2 peer-focus-visible:ring-blue/40 ${checked ? "bg-blue border-blue" : "bg-card border-line"}`}>
        {checked && (
          <svg viewBox="0 0 12 12" width="10" height="10" fill="none" stroke="#fff" strokeWidth="2" aria-hidden>
            <path d="M2.5 6.5l2.2 2.2 4.8-5" />
          </svg>
        )}
      </span>
      {children}
    </label>
  );
}

interface PinProps {
  id?: string;
  label: string;
  length: number;
  /** Most digits the input takes, when more than the boxes drawn. */
  max?: number;
  value: string;
  onChange: (v: string) => void;
  error?: boolean;
  disabled?: boolean;
  /** On a phone the on-screen keypad types, so the boxes do not raise the native keyboard. */
  keypad?: boolean;
  autoFocus?: boolean;
  onFull?: () => void;
}

/** Boxes with dots, over one real input so a hardware or native numeric keyboard types into it too. */
export function PinField({ id, label, length, max = length, value, onChange, error, disabled, keypad, autoFocus, onFull }: PinProps) {
  const ref = useRef<HTMLInputElement>(null);
  const [focused, setFocused] = useState(false);
  useEffect(() => {
    if (autoFocus && matchMedia(DESKTOP).matches) ref.current?.focus();
  }, [autoFocus]);
  const cur = disabled ? -1 : Math.min(value.length, length - 1);
  return (
    <div className="relative">
      <div className={`flex gap-[6px] justify-center ${disabled ? "opacity-45" : ""}`} aria-hidden>
        {Array.from({ length }, (_, i) => {
          const filled = i < value.length;
          const active = (focused || keypad) && i === cur && !filled;
          const border = error ? "border-straw" : active ? "border-blue ring-1 ring-blue" : "border-line";
          return (
            <i
              key={i}
              data-filled={filled}
              className={`flex-1 max-w-[46px] h-[42px] min-[721px]:h-[56px] grid place-items-center not-italic border bg-card rounded-button ${border}`}
            >
              {filled && <span className="w-[9px] h-[9px] rounded-full bg-ink" />}
            </i>
          );
        })}
      </div>
      <input
        ref={ref}
        id={id}
        aria-label={label}
        type="password"
        inputMode="numeric"
        autoComplete="off"
        pattern="[0-9]*"
        maxLength={max}
        disabled={disabled}
        value={value}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onChange={(e) => {
          const v = e.target.value.replace(/\D/g, "").slice(0, max);
          onChange(v);
          if (v.length === length) onFull?.();
        }}
        className={`absolute inset-0 w-full h-full opacity-0 cursor-text ${keypad ? "max-[720px]:pointer-events-none" : ""}`}
      />
    </div>
  );
}

const LETTERS: Record<string, string> = { "2": "ABC", "3": "DEF", "4": "GHI", "5": "JKL", "6": "MNO", "7": "PQRS", "8": "TUV", "9": "WXYZ" };

export function Keypad({ onDigit, onBackspace, disabled }: { onDigit: (d: string) => void; onBackspace: () => void; disabled?: boolean }) {
  const key = "h-[44px] rounded-[10px] bg-card border border-line flex flex-col items-center justify-center text-[20px] min-[721px]:text-[15px] font-semibold leading-none text-ink cursor-pointer disabled:cursor-default";
  return (
    <div className={`grid grid-cols-3 gap-[6px] min-[721px]:hidden ${disabled ? "opacity-45" : ""}`}>
      {["1", "2", "3", "4", "5", "6", "7", "8", "9"].map((d) => (
        <button key={d} type="button" disabled={disabled} onClick={() => onDigit(d)} className={key}>
          {d}
          {LETTERS[d] && <small className="text-[8px] min-[721px]:text-[6.5px] tracking-[.14em] text-mut mt-[2px] font-bold">{LETTERS[d]}</small>}
        </button>
      ))}
      <span />
      <button type="button" disabled={disabled} onClick={() => onDigit("0")} className={key}>
        0
      </button>
      <button type="button" disabled={disabled} onClick={onBackspace} aria-label="Delete" className="h-[44px] rounded-[10px] bg-transparent border-0 text-mut flex items-center justify-center cursor-pointer disabled:cursor-default">
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden>
          <path d="M9 6h11a1 1 0 011 1v10a1 1 0 01-1 1H9l-6-6z" />
          <path d="M13 9.5l4 4M17 9.5l-4 4" />
        </svg>
      </button>
    </div>
  );
}

export const LockButton = ({ onClick }: { onClick: () => void }) => (
  <Button variant="secondary" onClick={onClick} className="!px-[12px] !py-[5px] !text-[15px] min-[721px]:!text-[12.5px] !font-bold !leading-normal">
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden>
      <rect x="3" y="7" width="10" height="7" rx="1.5" />
      <path d="M5.5 7V5a2.5 2.5 0 015 0v2" />
    </svg>
    Lock
  </Button>
);

export function Back({ to, onClick }: { to: string; onClick: () => void }) {
  return (
    <button type="button" onClick={onClick} className="text-[14px] min-[721px]:text-[13px] text-mut bg-transparent border-0 p-[0px] cursor-pointer font-[inherit] self-start min-[721px]:self-auto">
      ← {to}
    </button>
  );
}

/** A device keeps the color the webapp gives it, in the library, in a meeting and in History alike. */
export function DeviceTag({ name, color }: { name: string; color: string }) {
  const tone = labelTone(color);
  return (
    <span className="text-[11px] min-[721px]:text-[9.5px] leading-normal font-bold px-[7px] py-[1px] rounded-pill whitespace-nowrap" style={{ background: tone.bg, color: tone.fg }}>
      {name}
    </span>
  );
}

const pill = (on: boolean) =>
  `flex-none text-[13px] min-[721px]:text-[11px] leading-normal font-bold px-[10px] py-[5px] rounded-pill border cursor-pointer font-[inherit] ${on ? "bg-blue border-blue text-white" : "bg-card border-line text-mut"}`;

const ROW = "flex gap-[6px] overflow-x-auto [scrollbar-width:none] min-[721px]:flex-wrap min-[721px]:overflow-visible";

/** A row of toggle pills: it scrolls sideways on a phone and wraps on a desktop. With `children` at its end it does neither, since scrolling would clip their popover. */
export function Pills<T>({ label, options, value, onChange, children }: { label: string; options: { value: T; label: string }[]; value: T | null; onChange: (v: T) => void; children?: ReactNode }) {
  return (
    <div
      role="group"
      aria-label={label}
      className={children ? "flex gap-[6px] items-center" : ROW}
    >
      {options.map((o) => (
        <button key={String(o.value)} type="button" aria-pressed={o.value === value} onClick={() => o.value !== value && onChange(o.value)} className={pill(o.value === value)}>
          {o.label}
        </button>
      ))}
      {children}
    </div>
  );
}

const LucideIcon = ({ d, className }: { d: string; className: string }) => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={className} aria-hidden>
    <path d={d} />
  </svg>
);

/** A pill at the end of the filter row that opens a device's labels as a list. Picking one closes the list, and the pill then shows it, and All labels takes it off. `labels` is null while loading, and opening the list after a failed load tries again. */
export function LabelMenu({ labels, error, onRetry, value, onChange, archived }: { labels: Label[] | null; error: string | null; onRetry: () => void; value: Label | null; onChange: (l: Label | null) => void; archived: boolean }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const close = () => {
    setOpen(false);
    button.current?.focus();
  };
  useEffect(() => {
    if (!open) return;
    const down = (e: PointerEvent) => !box.current?.contains(e.target as Node) && setOpen(false);
    // Captured on the document and stopped there, so the Escape that closes the list does not also clear the search or fold the chat.
    const key = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      close();
    };
    document.addEventListener("pointerdown", down);
    document.addEventListener("keydown", key, true);
    return () => {
      document.removeEventListener("pointerdown", down);
      document.removeEventListener("keydown", key, true);
    };
  }, [open]);
  const note = (text: string) => <p className="px-3 py-2 text-mut m-[0px]">{text}</p>;
  const option = (l: Label | null) => (
    <button
      key={l ? `l:${l.id}` : "all"}
      type="button"
      role="option"
      aria-selected={l?.id === value?.id}
      onClick={() => {
        close();
        if (l?.id !== value?.id) onChange(l);
      }}
      className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-paper cursor-pointer"
    >
      <span className="w-[9px] h-[9px] rounded-[3px] shrink-0" style={l ? { background: labelTone(l.color).fg } : undefined} />
      <span className={`flex-1 truncate ${l ? "text-ink" : "text-mut"}`}>{l ? l.name : "All labels"}</span>
      {l?.id === value?.id && <LucideIcon d="M20 6 9 17l-5-5" className="w-[14px] h-[14px] text-blue shrink-0" />}
    </button>
  );
  return (
    // Tabbing out closes the list. A null relatedTarget is a tap that Safari does not focus, which the pointerdown listener judges instead.
    <div ref={box} onBlur={(e) => e.relatedTarget && !e.currentTarget.contains(e.relatedTarget) && setOpen(false)} className="relative ml-auto min-w-0">
      <button
        ref={button}
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={value ? `Label ${value.name}` : "Labels"}
        onClick={() => {
          if (!open && error) onRetry();
          setOpen(!open);
        }}
        className={`${pill(!!value)} inline-flex items-center gap-[5px] max-w-full`}
      >
        {/* The ring keeps a blue label's swatch visible on the blue pill. */}
        {value && <span className="w-[8px] h-[8px] min-[721px]:w-[7px] min-[721px]:h-[7px] rounded-[2px] shrink-0 ring-1 ring-white/80" style={{ background: labelTone(value.color).fg }} />}
        <span className="truncate">{value ? value.name : "Labels"}</span>
        <LucideIcon d="m6 9 6 6 6-6" className="w-[14px] h-[14px] min-[721px]:w-[12px] min-[721px]:h-[12px] shrink-0" />
      </button>
      {open && (
        <div
          role="listbox"
          aria-label="Labels"
          className="absolute right-0 top-full mt-1 bg-card border border-line rounded-card shadow-pop py-1 w-[220px] max-h-[250px] overflow-y-auto z-40 text-[15px] min-[721px]:text-[12px]"
        >
          {error
            ? note(`Could not load labels. ${error}`)
            : !labels
              ? note("Loading labels…")
              : !labels.length && !value
                ? note(archived ? "No labels in this Mac's archive." : "No labels on this Mac.")
                : [option(null), ...labels.map(option)]}
        </div>
      )}
    </div>
  );
}

export interface DeviceEdits {
  recolor: (id: string, color: string) => Promise<void>;
  reorder: (ids: string[]) => Promise<void>;
}

/** Saves a device change, then reloads the device list. A rejected token locks the webapp, as on a read. */
export function deviceEdits(api: Api, token: string, reload: () => void, onRejected: () => void): DeviceEdits {
  const save = async (call: Promise<unknown>) => {
    try {
      await call;
    } catch (e) {
      if (e instanceof TokenRejected) onRejected();
      throw e;
    }
    reload();
  };
  return { recolor: (id, color) => save(api.recolorDevice(token, id, color)), reorder: (ids) => save(api.reorderDevices(token, ids)) };
}

/** "All devices" first, then one pill per device in its color. `value` is a device id, or null for all. Tapping the picked device again opens its settings. */
export function DevicePills({ devices, value, onChange, edits }: { devices: Device[]; value: string | null; onChange: (id: string | null) => void; edits: DeviceEdits }) {
  const [editing, setEditing] = useState<string | null>(null);
  const open = devices.find((d) => d.id === editing);
  return (
    <>
      <div role="group" aria-label="Device" className={ROW}>
        <button type="button" aria-pressed={value === null} onClick={() => value !== null && onChange(null)} className={pill(value === null)}>
          All devices
        </button>
        {devices.map((d) => {
          const on = d.id === value;
          const tone = labelTone(d.color);
          return (
            <button
              key={d.id}
              type="button"
              aria-pressed={on}
              aria-haspopup={on ? "dialog" : undefined}
              onClick={() => (on ? setEditing(d.id) : onChange(d.id))}
              className={`${pill(false)} inline-flex items-center gap-[5px]`}
              style={on ? { background: tone.bg, color: tone.fg, borderColor: tone.fg } : undefined}
            >
              {!on && <span aria-hidden className="w-[7px] h-[7px] rounded-full shrink-0" style={{ background: tone.fg }} />}
              {d.name}
              {on && <LucideIcon d="m6 9 6 6 6-6" className="w-[14px] h-[14px] min-[721px]:w-[12px] min-[721px]:h-[12px] shrink-0" />}
            </button>
          );
        })}
      </div>
      {open && <DeviceDialog device={open} devices={devices} edits={edits} onClose={() => setEditing(null)} />}
    </>
  );
}

const SWATCHES = Object.keys(LABEL_COLORS);
const SECTION = "text-[13px] min-[721px]:text-[11.5px] font-semibold text-mut m-[0px]";

/** A native dialog, because the device row scrolls on a phone and would clip a popover. */
function DeviceDialog({ device, devices, edits, onClose }: { device: Device; devices: Device[]; edits: DeviceEdits; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  const [view, setView] = useState<"color" | "order">("color");
  const [color, setColor] = useState(device.color);
  const [hex, setHex] = useState(isHexColor(device.color) ? device.color : "");
  const [error, setError] = useState<string | null>(null);
  const timer = useRef(0);
  const savedColor = useRef(device.color);
  // One save at a time, so two quick changes reach the database in the order they were made.
  // Only the newest save's failure is shown and undone, since a later save already replaced an earlier one.
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const latest = useRef(0);
  const serial = (run: () => Promise<void>, what: string) => {
    setError(null);
    const n = ++latest.current;
    const done = queue.current.then(run).then(
      () => true,
      (e: Error) => {
        if (n === latest.current) setError(`Could not save the ${what}. ${e.message}`);
        return false;
      },
    );
    queue.current = done;
    return done.then((ok) => ({ ok, newest: n === latest.current }));
  };
  const backdrop = useRef(false);
  useEffect(() => {
    ref.current?.showModal();
  }, []);
  const close = () => ref.current?.close();
  const save = (c: string) => {
    window.clearTimeout(timer.current);
    setColor(c);
    void serial(() => edits.recolor(device.id, c), "color").then(({ ok, newest }) => {
      if (ok) savedColor.current = c;
      else if (newest) {
        setColor(savedColor.current);
        setHex(isHexColor(savedColor.current) ? savedColor.current : "");
      }
    });
  };
  const reorder = (ids: string[]) => serial(() => edits.reorder(ids), "order");
  const swatch = "w-[30px] h-[30px] min-[721px]:w-[22px] min-[721px]:h-[22px] rounded-full border-0 p-[0px] cursor-pointer";
  const picked = "ring-2 ring-offset-2 ring-offset-card ring-ink";
  return (
    <dialog
      ref={ref}
      aria-label={view === "color" ? device.name : "Reorder devices"}
      onClose={onClose}
      // Only a press that starts and ends on the backdrop closes it, so a drag out of the hex field does not.
      onPointerDown={(e) => (backdrop.current = e.target === e.currentTarget)}
      onClick={(e) => backdrop.current && e.target === e.currentTarget && close()}
      // The Escape that closes the dialog must not also fold the chat.
      onKeyDown={(e) => e.key === "Escape" && e.stopPropagation()}
      className="m-auto p-[0px] w-[min(360px,calc(100vw-32px))] rounded-panel border border-line bg-card text-ink shadow-window backdrop:bg-black/30"
    >
      <div className="flex flex-col gap-[14px] p-[18px] text-[15px] min-[721px]:text-[12.5px]">
        {view === "color" ? (
          <>
            <h2 className="text-[19px] min-[721px]:text-[15px] font-extrabold m-[0px] truncate">{device.name}</h2>
            <div className="flex flex-col gap-[10px]">
              <p className={SECTION}>Color</p>
              <div role="radiogroup" aria-label="Color" className="flex flex-wrap items-center gap-[12px] min-[721px]:gap-[10px]">
                {SWATCHES.map((c) => (
                  <button
                    key={c}
                    type="button"
                    role="radio"
                    aria-checked={color === c}
                    aria-label={c}
                    onClick={() => color !== c && save(c)}
                    className={`${swatch} ${color === c ? picked : ""}`}
                    style={{ background: labelTone(c).fg }}
                  />
                ))}
              </div>
              <div className="flex items-center gap-[8px]">
                {/* A wheel until a custom color is picked, then that color. The native input sits invisibly on top to open the system picker. */}
                <span
                  className={`${swatch} relative shrink-0 ${isHexColor(color) ? picked : ""}`}
                  style={{ background: isHexColor(color) ? color : "conic-gradient(#e05a5a, #e0b05a, #6cc070, #5ab4e0, #7a6ae0, #d05ab4, #e05a5a)" }}
                >
                  <input
                    type="color"
                    aria-label="Custom color"
                    value={isHexColor(color) ? color : DEFAULT_CUSTOM_COLOR}
                    // The picker reports every step of a drag, so only the color it rests on is saved.
                    onChange={(e) => {
                      const c = e.target.value;
                      setColor(c);
                      setHex(c);
                      window.clearTimeout(timer.current);
                      timer.current = window.setTimeout(() => save(c), 400);
                    }}
                    className="absolute inset-0 w-full h-full opacity-0 cursor-pointer"
                  />
                </span>
                <input
                  type="text"
                  aria-label="Custom color hex"
                  value={hex}
                  placeholder="#a3c9ff"
                  onChange={(e) => setHex(e.target.value)}
                  onBlur={() => isHexColor(hex) && hex.toLowerCase() !== color && save(hex.toLowerCase())}
                  onKeyDown={(e) => e.key === "Enter" && isHexColor(hex) && hex.toLowerCase() !== color && save(hex.toLowerCase())}
                  autoComplete="off"
                  spellCheck={false}
                  className="w-[104px] min-[721px]:w-[84px] px-[8px] py-[4px] rounded-button border border-line bg-paper font-mono text-[15px] min-[721px]:text-[11.5px] text-ink outline-none focus:border-blue"
                />
              </div>
            </div>
            {devices.length > 1 && (
              <button
                type="button"
                onClick={() => {
                  setView("order");
                  setError(null);
                }}
                className="flex items-center justify-between border-0 border-t border-line bg-transparent pt-[12px] px-[0px] pb-[0px] text-left font-[inherit] text-[inherit] font-semibold text-ink cursor-pointer"
              >
                Reorder devices…
                <LucideIcon d="m9 18 6-6-6-6" className="w-[16px] h-[16px] text-mut shrink-0" />
              </button>
            )}
          </>
        ) : (
          <>
            <h2 className="text-[19px] min-[721px]:text-[15px] font-extrabold m-[0px]">Reorder devices</h2>
            <ReorderList devices={devices} onSave={reorder} />
          </>
        )}
        {error && (
          <p role="alert" className="text-straw m-[0px]">
            {error}
          </p>
        )}
        <div className="flex justify-end">
          <Button size="sm" onClick={close}>
            Done
          </Button>
        </div>
      </div>
    </dialog>
  );
}

const moveId = (ids: string[], id: string, by: number) => {
  const from = ids.indexOf(id);
  const to = Math.max(0, Math.min(ids.length - 1, from + by));
  const next = ids.filter((x) => x !== id);
  next.splice(to, 0, id);
  return next;
};

/** Drag a row by its handle, or focus the handle and press the arrow keys. Each drop saves the whole order, and a failed save puts back the last order that saved. */
function ReorderList({ devices, onSave }: { devices: Device[]; onSave: (ids: string[]) => Promise<{ ok: boolean; newest: boolean }> }) {
  const [order, setOrderState] = useState(() => devices.map((d) => d.id));
  const current = useRef(order);
  const saved = useRef(order);
  const setOrder = (ids: string[]) => {
    current.current = ids;
    setOrderState(ids);
  };
  const [dragging, setDragging] = useState<string | null>(null);
  const handles = useRef(new Map<string, HTMLButtonElement>());
  const hint = useId();
  const desktop = useDesktop();
  const byId = new Map(devices.map((d) => [d.id, d]));

  const commit = (ids: string[]) =>
    void onSave(ids).then(({ ok, newest }) => {
      if (ok) saved.current = ids;
      else if (newest) setOrder(saved.current);
    });
  // The window follows the drag, because React moves the dragged row's node and the handle can lose pointer capture.
  const down = (e: ReactPointerEvent<HTMLButtonElement>, id: string) => {
    const row = e.currentTarget.closest("li");
    if (!row) return;
    e.preventDefault();
    // A row plus the list's gap is one step.
    const step = row.offsetHeight + 6;
    let y = e.clientY;
    let moved = false;
    setDragging(id);
    const move = (ev: PointerEvent) => {
      const by = Math.round((ev.clientY - y) / step);
      if (!by) return;
      const before = current.current;
      const next = moveId(before, id, by);
      const shift = next.indexOf(id) - before.indexOf(id);
      if (!shift) return;
      y += shift * step;
      moved = true;
      setOrder(next);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      setDragging(null);
      if (moved) commit(current.current);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
  };
  const key = (e: React.KeyboardEvent, id: string) => {
    const by = e.key === "ArrowUp" ? -1 : e.key === "ArrowDown" ? 1 : 0;
    if (!by) return;
    e.preventDefault();
    const next = moveId(current.current, id, by);
    if (next.indexOf(id) === current.current.indexOf(id)) return;
    setOrder(next);
    commit(next);
    // Moving a focused node can drop its focus.
    requestAnimationFrame(() => handles.current.get(id)?.focus());
  };

  return (
    <>
      <p id={hint} className="text-mut m-[0px] text-[13px] min-[721px]:text-[11.5px]">
        {desktop ? "Drag a device by its handle, or focus the handle and use the arrow keys." : "Drag a device by its handle."}
      </p>
      <ol aria-label="Devices" className="list-none m-[0px] p-[0px] flex flex-col gap-[6px]">
        {order.map((id) => {
          const d = byId.get(id);
          if (!d) return null;
          return (
            <li
              key={id}
              className={`flex items-center gap-[10px] pl-[4px] pr-[12px] py-[4px] rounded-button border ${dragging === id ? "border-blue bg-paper shadow-pop" : "border-line bg-card"}`}
            >
              <button
                ref={(el) => {
                  if (el) handles.current.set(id, el);
                  else handles.current.delete(id);
                }}
                type="button"
                aria-label={`Move ${d.name}`}
                aria-describedby={hint}
                onPointerDown={(e) => down(e, id)}
                onKeyDown={(e) => key(e, id)}
                className="grid place-items-center w-[36px] h-[36px] min-[721px]:w-[28px] min-[721px]:h-[28px] rounded-button border-0 bg-transparent text-mut touch-none cursor-grab active:cursor-grabbing"
              >
                <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden>
                  {[4, 8, 12].flatMap((y) => [<circle key={`a${y}`} cx="6" cy={y} r="1.2" />, <circle key={`b${y}`} cx="10" cy={y} r="1.2" />])}
                </svg>
              </button>
              <span aria-hidden className="w-[9px] h-[9px] rounded-full shrink-0" style={{ background: labelTone(d.color).fg }} />
              <span className="flex-1 min-w-0 truncate font-semibold">{d.name}</span>
            </li>
          );
        })}
      </ol>
    </>
  );
}

export function SearchField({ value, onChange, inputRef }: { value: string; onChange: (v: string) => void; inputRef: React.RefObject<HTMLInputElement | null> }) {
  const clear = () => {
    onChange("");
    inputRef.current?.focus();
  };
  return (
    <div role="search" className="flex items-center gap-[7px] bg-card border border-line rounded-pill px-[12px] py-[6px] focus-within:border-blue focus-within:ring-1 focus-within:ring-blue">
      <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" className="text-mut flex-none" aria-hidden>
        <circle cx="7" cy="7" r="4.5" />
        <path d="M10.5 10.5L14 14" />
      </svg>
      <input
        ref={inputRef}
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => e.key === "Escape" && !e.nativeEvent.isComposing && clear()}
        placeholder="Search notes & transcripts"
        aria-label="Search notes and transcripts"
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        className="flex-1 min-w-0 bg-transparent border-0 outline-none font-sans text-[16px] min-[721px]:text-[12px] text-ink placeholder:text-mut p-[0px] pr-[10px]"
      />
      {value && (
        <button
          type="button"
          aria-label="Clear search"
          onClick={clear}
          className="relative flex-none grid place-items-center w-[16px] h-[16px] rounded-full border-0 bg-line text-mut cursor-pointer p-[0px] before:absolute before:-inset-[14px]"
        >
          <svg viewBox="0 0 10 10" width="7" height="7" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden>
            <path d="M2 2l6 6M8 2l-6 6" />
          </svg>
        </button>
      )}
    </div>
  );
}

/** A tappable card: bold title, then a muted line on the left and a device tag on the right. A search hit adds a snippet with the match marked. */
export function ListRow({ title, meta, tag, snippet, selected, onClick }: { title: string; meta: string; tag: ReactNode; snippet?: string | null; selected?: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      data-testid="list-row"
      aria-current={selected || undefined}
      className={`block w-full text-left font-[inherit] text-ink bg-card border rounded-button px-[11px] py-[9px] cursor-pointer ${selected ? "border-blue shadow-[0_0_0_1px_var(--color-blue)]" : "border-line"}`}
    >
      <b className="block text-[16px] min-[721px]:text-[12px] font-bold truncate">{title}</b>
      <span className="flex justify-between items-center gap-[6px] text-[13px] min-[721px]:text-[10px] text-mut mt-[2px]">
        <span>{meta}</span>
        {tag}
      </span>
      {snippet && (
        <span className="text-[13.5px] min-[721px]:text-[10.5px] leading-snug text-mut mt-[4px] line-clamp-2">
          {snippetParts(snippet).map((p, i) =>
            p.hit ? (
              <mark key={i} className="bg-blsoft text-ink font-semibold rounded-[3px]">
                {p.text}
              </mark>
            ) : (
              p.text
            ),
          )}
        </span>
      )}
    </button>
  );
}

/** Rows under day labels. `items` come from the shared day grouping. */
export function DayList<T>({ groups, render }: { groups: { key: number; label: string; meetings: T[] }[]; render: (item: T) => ReactNode }) {
  return (
    <>
      {groups.map((g) => (
        <section key={g.key} className="flex flex-col gap-[10px] min-[721px]:gap-[8px]">
          <h2 className="font-mono text-[11px] min-[721px]:text-[9.5px] font-normal uppercase tracking-[.08em] text-mut mt-[4px] m-[0px]">{g.label}</h2>
          {g.meetings.map(render)}
        </section>
      ))}
    </>
  );
}

/** Previous and Next for results with no known total, styled like the shared library Pager. */
export function SearchPager({ canPrevious, canNext, onPrevious, onNext }: { canPrevious: boolean; canNext: boolean; onPrevious: () => void; onNext: () => void }) {
  return (
    <nav aria-label="Pagination" className="mt-4 flex items-center justify-end gap-2">
      <button type="button" className={PAGER_BUTTON} disabled={!canPrevious} onClick={onPrevious}>
        Previous
      </button>
      <button type="button" className={PAGER_BUTTON} disabled={!canNext} onClick={onNext}>
        Next
      </button>
    </nav>
  );
}
