import { useEffect, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { Button } from "@web/components/Button";
import { Logo } from "@web/components/Logo";

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

export const h3 = "text-[20px] font-extrabold tracking-[-0.01em] text-ink m-[0px]";
export const sub = "text-[12px] min-[721px]:text-[12.5px] text-mut m-[0px]";
const kLabel = "font-mono text-[10px] uppercase tracking-[.08em] text-mut";

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
    <div role="alert" className="flex gap-[8px] items-start text-[12px] rounded-button px-[12px] py-[9px] bg-strsoft text-straw">
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
      <Button variant="secondary" onClick={onRetry} className="!px-[14px] !py-[8px] !text-[12.5px] !font-bold !leading-normal">
        Retry
      </Button>
    </div>
  );
}

export function TextLink({ lead, children, onClick }: { lead?: string; children: ReactNode; onClick: () => void }) {
  return (
    <div className="text-[11.5px] min-[721px]:text-[12.5px] font-semibold text-center text-mut">
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
          className={`font-sans text-[11px] font-semibold px-[12px] py-[3px] rounded-[9px] cursor-pointer border-0 ${o.value === value ? "bg-card text-ink shadow-[0_2px_6px_rgba(40,30,15,.10)]" : "bg-transparent text-mut"}`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Checkbox({ checked, onChange, children }: { checked: boolean; onChange: (v: boolean) => void; children: ReactNode }) {
  return (
    <label className="flex items-center gap-[8px] text-[11.5px] min-[721px]:text-[13px] cursor-pointer">
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
  length: 4 | 6;
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
export function PinField({ id, label, length, value, onChange, error, disabled, keypad, autoFocus, onFull }: PinProps) {
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
        maxLength={length}
        disabled={disabled}
        value={value}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onChange={(e) => {
          const v = e.target.value.replace(/\D/g, "").slice(0, length);
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
  const key = "h-[44px] rounded-[10px] bg-card border border-line flex flex-col items-center justify-center text-[15px] font-semibold leading-none text-ink cursor-pointer disabled:cursor-default";
  return (
    <div className={`grid grid-cols-3 gap-[6px] min-[721px]:hidden ${disabled ? "opacity-45" : ""}`}>
      {["1", "2", "3", "4", "5", "6", "7", "8", "9"].map((d) => (
        <button key={d} type="button" disabled={disabled} onClick={() => onDigit(d)} className={key}>
          {d}
          {LETTERS[d] && <small className="text-[6.5px] tracking-[.14em] text-mut mt-[2px] font-bold">{LETTERS[d]}</small>}
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
