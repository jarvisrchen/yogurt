import { useMemo, useState } from "react";
import { groupMeetingsByDay } from "@web/components/library/dayGroups";
import { Logo } from "@web/components/Logo";
import { Brand, DayList, DevicePills, DeviceTag, ListRow, LoadError, LockButton, Note, SearchField, useDesktop } from "../ui";
import type { Api } from "../lib/api";
import { asOfLabel, clock, duration, forDevice, toEntries } from "../lib/library";
import { useSearch } from "../lib/useSearch";
import { useCloudLoad } from "../lib/useCloudLoad";
import { HOME, type Loc } from "../lib/hash";
import { MeetingView } from "./Meeting";

export const GearIcon = () => (
  <svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" aria-hidden>
    <circle cx="8" cy="8" r="2.2" />
    <path d="M6.40 2.84L6.83 1.20L9.17 1.20L9.60 2.84L10.52 3.22L11.98 2.37L13.63 4.02L12.78 5.48L13.16 6.40L14.80 6.83L14.80 9.17L13.16 9.60L12.78 10.52L13.63 11.98L11.98 13.63L10.52 12.78L9.60 13.16L9.17 14.80L6.83 14.80L6.40 13.16L5.48 12.78L4.02 13.63L2.37 11.98L3.22 10.52L2.84 9.60L1.20 9.17L1.20 6.83L2.84 6.40L3.22 5.48L2.37 4.02L4.02 2.37L5.48 3.22Z" />
  </svg>
);

interface Props {
  api: Api;
  token: string;
  loc: Loc;
  go: (to: Loc) => void;
  back: (to: Loc) => void;
  onSettings: () => void;
  onLock: () => void;
  onRejected: () => void;
}

export function Library({ api, token, loc, go, back, onSettings, onLock, onRejected }: Props) {
  const desktop = useDesktop();
  const [device, setDevice] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const { asOf: pushId, meeting: open } = loc;
  const { data, error, retry } = useCloudLoad(
    async () => {
      const [devices, rows, history] = await Promise.all([api.devices(token), api.library(token, pushId ?? undefined), pushId ? api.history(token) : null]);
      return { devices, entries: toEntries(rows), pushedAt: history?.find((h) => h.push_id === pushId)?.pushed_at ?? null };
    },
    onRejected,
    [api, token, pushId],
  );
  const devices = data?.devices;
  const entries = data?.entries;

  const search = useSearch(api, token, pushId, query, onRejected);
  const snippets = useMemo(() => new Map(search.hits?.map((h) => [`${h.device_id}/${h.meeting_id}`, h.snippet])), [search.hits]);
  const shown = useMemo(() => {
    const inDevice = forDevice(entries ?? [], device);
    return search.active && search.hits ? inDevice.filter((e) => snippets.has(e.key)) : inDevice;
  }, [entries, device, search.active, search.hits, snippets]);
  const groups = useMemo(() => groupMeetingsByDay(shown), [shown]);
  const opened = shown.find((e) => e.key === open) ?? null;
  const indexOf = (id: string) => Math.max(0, devices?.findIndex((d) => d.id === id) ?? 0);
  const nameOf = (id: string) => devices?.find((d) => d.id === id)?.name ?? "Unknown device";

  const meeting = opened && (
    <MeetingView
      key={opened.key}
      entry={opened}
      device={nameOf(opened.device_id)}
      deviceIndex={indexOf(opened.device_id)}
      back={devices?.find((d) => d.id === device)?.name ?? "All devices"}
      onBack={() => back({ ...loc, meeting: null })}
      desktop={desktop}
    />
  );
  if (!desktop && open && !entries && !error) return null;
  if (!desktop && meeting) return <main className="min-h-dvh flex flex-col px-[16px] pt-[16px] pb-[16px]">{meeting}</main>;

  const list = (
    <>
      {pushId && (
        <div className="flex items-center gap-[7px] pl-[10px] pr-[4px] py-[4px] bg-card border border-line rounded-pill shadow-[0_8px_18px_-6px_var(--shadow-card-color)] text-[11px] min-w-0">
          <span className="w-[6px] h-[6px] rounded-full bg-blue flex-none" />
          <span className="flex-1 min-w-0 truncate">
            <span className="text-mut">Viewing </span>
            <b className="font-semibold">{data?.pushedAt ? asOfLabel(data.pushedAt) : `push ${pushId}`}</b>
          </span>
          <button type="button" onClick={() => go(HOME)} className="flex-none bg-blue text-white font-semibold text-[10.5px] leading-normal px-[9px] py-[4px] rounded-pill border-0 cursor-pointer font-[inherit]">
            Latest
          </button>
        </div>
      )}
      <SearchField value={query} onChange={setQuery} />
      {!!devices?.length && <DevicePills devices={devices} value={device} onChange={setDevice} />}
      <DayList
        groups={groups}
        render={(e) => (
          <ListRow
            key={e.key}
            title={e.title}
            meta={`${clock(e.started_at)}${duration(e) ? ` · ${duration(e)}` : ""}`}
            tag={<DeviceTag name={nameOf(e.device_id)} index={indexOf(e.device_id)} />}
            snippet={snippets.get(e.key)}
            selected={e.key === open}
            onClick={() => go({ ...loc, meeting: e.key })}
          />
        )}
      />
      {search.error && <Note>{`Search failed. ${search.error}`}</Note>}
      {entries && !shown.length && <p className="text-[12px] text-mut m-[0px]">{search.active ? `No meetings match "${query.trim()}".` : "No meetings here."}</p>}
    </>
  );

  return (
    <div className="min-h-dvh flex flex-col">
      <header className="flex items-center justify-between gap-[10px] px-[16px] py-[16px] min-[721px]:px-[20px] min-[721px]:py-[12px] min-[721px]:bg-card min-[721px]:border-b min-[721px]:border-line">
        <Brand />
        <div className="flex items-center gap-[6px] min-[721px]:gap-[8px]">
          <span className="text-[10px] min-[721px]:text-[11px] font-bold px-[10px] py-[3px] rounded-pill bg-slatesoft text-slate">read-only</span>
          <button type="button" aria-label="Settings" onClick={onSettings} className="w-[24px] h-[24px] min-[721px]:w-[30px] min-[721px]:h-[30px] grid place-items-center rounded-full border border-line bg-card text-mut cursor-pointer">
            <GearIcon />
          </button>
          {desktop && <LockButton onClick={onLock} />}
        </div>
      </header>
      {error ? (
        <main className="flex-1 flex flex-col items-center px-[16px] pt-[80px]">
          <LoadError message={`Could not load your library. ${error}`} onRetry={retry} />
        </main>
      ) : !devices || !entries ? null : !entries.length && !pushId ? (
        <main className="flex-1 flex flex-col items-center text-center px-[16px] pt-[80px]">
          <div className="float-3500 mb-[32px]">
            <Logo size={64} ariaLabel="Yogurt" />
          </div>
          <h2 className="heading-state mb-[12px]">{devices.length ? "Waiting for the first push" : "No machine has pushed yet"}</h2>
          <p className="text-[15px] text-mut max-w-md m-0">
            {devices.length
              ? `${devices.length} ${devices.length === 1 ? "machine is" : "machines are"} connected. Meetings show here after a push.`
              : "Connect a laptop in yogurt's Settings under Cloud sync, then push your meetings. They show up here."}
          </p>
        </main>
      ) : desktop ? (
        <div className="flex-1 grid grid-cols-[300px_minmax(0,1fr)]">
          <aside className="border-r border-line p-[14px] flex flex-col gap-[8px]">{list}</aside>
          <main className="px-[28px] py-[22px] flex flex-col">
            {meeting ?? <p className="text-[13px] text-mut m-[0px]">Pick a meeting to read it.</p>}
          </main>
        </div>
      ) : (
        <main className="flex-1 flex flex-col gap-[10px] px-[16px] pb-[16px]">{list}</main>
      )}
    </div>
  );
}
