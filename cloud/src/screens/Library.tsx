import { useEffect, useState } from "react";
import { Button } from "@web/components/Button";
import { Logo } from "@web/components/Logo";
import { Brand, LoadError, useDesktop } from "../ui";
import { TokenRejected, type Api, type Device } from "../lib/api";

export const GearIcon = () => (
  <svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden>
    <circle cx="8" cy="8" r="2.2" />
    <path d="M8 1.8v1.6M8 12.6v1.6M1.8 8h1.6M12.6 8h1.6M3.6 3.6l1.1 1.1M11.3 11.3l1.1 1.1M3.6 12.4l1.1-1.1M11.3 4.7l1.1-1.1" />
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
