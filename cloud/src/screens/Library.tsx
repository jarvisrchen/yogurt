import { useEffect, useState } from "react";
import { Button } from "@web/components/Button";
import { Logo } from "@web/components/Logo";
import { Brand, LoadError, useDesktop } from "../ui";
import { TokenRejected, type Api, type Device } from "../lib/api";

export const GearIcon = () => (
  <svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" aria-hidden>
    <circle cx="8" cy="8" r="2.2" />
    <path d="M6.40 2.84L6.83 1.20L9.17 1.20L9.60 2.84L10.52 3.22L11.98 2.37L13.63 4.02L12.78 5.48L13.16 6.40L14.80 6.83L14.80 9.17L13.16 9.60L12.78 10.52L13.63 11.98L11.98 13.63L10.52 12.78L9.60 13.16L9.17 14.80L6.83 14.80L6.40 13.16L5.48 12.78L4.02 13.63L2.37 11.98L3.22 10.52L2.84 9.60L1.20 9.17L1.20 6.83L2.84 6.40L3.22 5.48L2.37 4.02L4.02 2.37L5.48 3.22Z" />
  </svg>
);

export const LockSmall = () => (
  <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden>
    <rect x="3" y="7" width="10" height="7" rx="1.5" />
    <path d="M5.5 7V5a2.5 2.5 0 015 0v2" />
  </svg>
);

export function Library({ api, token, onSettings, onLock, onRejected }: { api: Api; token: string; onSettings: () => void; onLock: () => void; onRejected: () => void }) {
  const desktop = useDesktop();
  const [devices, setDevices] = useState<Device[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    setError(null);
    api.devices(token).then(setDevices, (e) => (e instanceof TokenRejected ? onRejected() : setError(e.message)));
  }, [api, token, onRejected, attempt]);

  return (
    <div className="min-h-dvh flex flex-col">
      <header className="flex items-center justify-between gap-[10px] px-[16px] py-[16px] min-[721px]:px-[20px] min-[721px]:py-[12px] min-[721px]:bg-card min-[721px]:border-b min-[721px]:border-line">
        <Brand />
        <div className="flex items-center gap-[6px] min-[721px]:gap-[8px]">
          <span className="text-[10px] min-[721px]:text-[11px] font-bold px-[10px] py-[3px] rounded-pill bg-slatesoft text-slate">read-only</span>
          <button type="button" aria-label="Settings" onClick={onSettings} className="w-[24px] h-[24px] min-[721px]:w-[30px] min-[721px]:h-[30px] grid place-items-center rounded-full border border-line bg-card text-mut cursor-pointer">
            <GearIcon />
          </button>
          {desktop && (
            <Button variant="secondary" onClick={onLock} className="!px-[12px] !py-[5px] !text-[12.5px] !font-bold !leading-normal">
              <LockSmall />
              Lock
            </Button>
          )}
        </div>
      </header>
      <main className="flex-1 flex flex-col items-center text-center px-[16px] pt-[80px]">
        {error ? (
          <LoadError message={`Could not load your library. ${error}`} onRetry={() => setAttempt((n) => n + 1)} />
        ) : (
          <>
            <div className="float-3500 mb-[32px]">
              <Logo size={64} ariaLabel="Yogurt" />
            </div>
            <h2 className="heading-state mb-[12px]">{devices?.length ? "Waiting for the first push" : "No machine has pushed yet"}</h2>
            <p className="text-[15px] text-mut max-w-md m-0">
              {devices?.length
                ? `${devices.length} ${devices.length === 1 ? "machine is" : "machines are"} connected. Meetings show here after a push.`
                : "Connect a laptop in yogurt's Settings under Cloud sync, then push your meetings. They show up here."}
            </p>
          </>
        )}
      </main>
    </div>
  );
}
