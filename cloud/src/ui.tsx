import { useEffect, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { Button } from "@web/components/Button";
import { Logo } from "@web/components/Logo";
import { PAGER_BUTTON } from "@web/components/library/Pager";
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

const TONES = ["bg-blsoft text-blue", "bg-honeysoft text-honey", "bg-slatesoft text-slate"];

/** A device keeps one colour by its place in the device list, in the library, in a meeting and in History alike. */
export function DeviceTag({ name, index }: { name: string; index: number }) {
  return <span className={`text-[11px] min-[721px]:text-[9.5px] leading-normal font-bold px-[7px] py-[1px] rounded-pill whitespace-nowrap ${TONES[index % TONES.length]}`}>{name}</span>;
}

/** A row of toggle pills: it scrolls sideways on a phone and wraps on a desktop. */
export function Pills<T>({ label, options, value, onChange }: { label: string; options: { value: T; label: string }[]; value: T; onChange: (v: T) => void }) {
  const pill = (on: boolean) =>
    `flex-none text-[13px] min-[721px]:text-[11px] leading-normal font-bold px-[10px] py-[5px] rounded-pill border cursor-pointer font-[inherit] ${on ? "bg-blue border-blue text-white" : "bg-card border-line text-mut"}`;
  return (
    <div role="group" aria-label={label} className="flex gap-[6px] overflow-x-auto [scrollbar-width:none] min-[721px]:flex-wrap min-[721px]:overflow-visible">
      {options.map((o) => (
        <button key={String(o.value)} type="button" aria-pressed={o.value === value} onClick={() => onChange(o.value)} className={pill(o.value === value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** "All devices" first, then one pill per device. `value` is a device id, or null for all. */
export function DevicePills({ devices, value, onChange }: { devices: { id: string; name: string }[]; value: string | null; onChange: (id: string | null) => void }) {
  return <Pills label="Device" options={[{ value: null, label: "All devices" }, ...devices.map((d) => ({ value: d.id, label: d.name }))]} value={value} onChange={onChange} />;
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
