import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { groupMeetingsByDay } from "@web/components/library/dayGroups";
import { Pager } from "@web/components/library/Pager";
import { rememberPageSize, resolvePageSize } from "@web/components/library/pagination";
import { Logo } from "@web/components/Logo";
import { ChatSources } from "@web/components/ChatSources";
import { LIBRARY_CHAT_COPY } from "@web/components/ChatWindow";
import {
  Brand,
  DayList,
  DevicePills,
  DeviceTag,
  LabelMenu,
  ListRow,
  LoadError,
  LockButton,
  Pills,
  SearchField,
  SearchPager,
  useDesktop,
} from "../ui";
import type { Api, Label } from "../lib/api";
import {
  asOfLabel,
  clock,
  duration,
  hitEntry,
  summaryEntry,
  toEntry,
  type Entry,
} from "../lib/library";
import { SEARCH_PAGE, searchStatus } from "../lib/search";
import { useSearch } from "../lib/useSearch";
import { useCloudLoad } from "../lib/useCloudLoad";
import { format, HOME, type Loc } from "../lib/hash";
import { Ask, DOCKED, MeetingView, type BodyState } from "./Meeting";
import { useChat } from "../lib/useChat";

export const GearIcon = () => (
  <svg
    viewBox="0 0 16 16"
    width="15"
    height="15"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinejoin="round"
    aria-hidden
  >
    <circle cx="8" cy="8" r="2.2" />
    <path d="M6.40 2.84L6.83 1.20L9.17 1.20L9.60 2.84L10.52 3.22L11.98 2.37L13.63 4.02L12.78 5.48L13.16 6.40L14.80 6.83L14.80 9.17L13.16 9.60L12.78 10.52L13.63 11.98L11.98 13.63L10.52 12.78L9.60 13.16L9.17 14.80L6.83 14.80L6.40 13.16L5.48 12.78L4.02 13.63L2.37 11.98L3.22 10.52L2.84 9.60L1.20 9.17L1.20 6.83L2.84 6.40L3.22 5.48L2.37 4.02L4.02 2.37L5.48 3.22Z" />
  </svg>
);

type Row = Entry & { snippet: string | null };

// Starred and labels belong to one Mac, so they are offered only once a Mac is picked.
type Pill = "all" | "starred" | "archive";
type Filter = Pill | Label;
const ALL = { value: "all", label: "All" } as const;
const ARCHIVE = { value: "archive", label: "Archive" } as const;
const FILTERS: { value: Pill; label: string }[] = [ALL, ARCHIVE];
const DEVICE_FILTERS: { value: Pill; label: string }[] = [ALL, { value: "starred", label: "Starred" }, ARCHIVE];

interface Props {
  api: Api;
  token: string;
  loc: Loc;
  go: (to: Loc) => void;
  replace: (to: Loc) => void;
  back: (to: Loc) => void;
  onSettings: () => void;
  onLock: () => void;
  onRejected: () => void;
}

export function Library({
  api,
  token,
  loc,
  go,
  replace,
  back,
  onSettings,
  onLock,
  onRejected,
}: Props) {
  const desktop = useDesktop();
  const [device, setDevice] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const archived = filter === "archive";
  const starred = filter === "starred";
  const label = typeof filter === "object" ? filter : null;
  const scope = { device, archived, starred, label: label?.id ?? null };
  const [query, setQuery] = useState("");
  const { asOf: pushId, meeting: open, page } = loc;
  const size = loc.size ?? resolvePageSize(null);

  const base = useCloudLoad(
    async () => {
      const [devices, history] = await Promise.all([
        api.devices(token),
        pushId ? api.history(token) : null,
      ]);
      return {
        devices,
        pushedAt: history?.find((h) => h.push_id === pushId)?.pushed_at ?? null,
      };
    },
    onRejected,
    [api, token, pushId],
  );
  const devices = base.data?.devices;

  // One page at a time. The last page stays up, dimmed, while the next loads.
  const pageLoad = useCloudLoad(
    async () => {
      const rows = await api.library(token, {
        atPush: pushId,
        limit: size,
        offset: (page - 1) * size,
        ...scope,
      });
      // An empty library shows the welcome screen, unless every meeting is archived and the Archive pill is the way to them.
      const empty =
        !rows.length && page === 1 && !pushId && !device && !archived &&
        !(await api.library(token, { atPush: null, limit: 1, offset: 0, device: null, archived: true, starred: false, label: null })).length;
      return { entries: rows.map(summaryEntry), total: rows[0]?.total ?? 0, archived, empty };
    },
    onRejected,
    [api, token, pushId, size, page, device, archived, starred, scope.label],
  );
  const labels = useCloudLoad(async () => (device ? api.labels(token, device) : null), onRejected, [api, token, device]);
  const shown = pageLoad.data ?? pageLoad.previous;
  const turning = pageLoad.loading && !pageLoad.data;

  // A page past the end has no rows to say how many there are, so start over.
  const stale = !!pageLoad.data && !pageLoad.data.entries.length && page > 1;
  useEffect(() => {
    if (stale) replace({ ...loc, page: 1 });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stale]);

  const field = useRef<HTMLInputElement>(null);
  // The as-of view has no search, so the hook gets an empty query there.
  const search = useSearch(api, token, pushId ? "" : query, scope, onRejected);
  const hits = search.active
    ? search.pending
      ? search.previous
      : search.hits
    : null;
  const rows = useMemo<Row[]>(
    () =>
      hits
        ? hits.map((h) => ({ ...hitEntry(h), snippet: h.snippet }))
        : (shown?.entries ?? []).map((e) => ({ ...e, snippet: null })),
    [hits, shown],
  );
  const groups = useMemo(() => groupMeetingsByDay(rows), [rows]);

  // Opening a meeting, from a page or a search hit, fetches that one body.
  const bodyLoad = useCloudLoad(
    async () => {
      if (!open) return null;
      const i = open.indexOf("/");
      const deviceId = open.slice(0, i);
      const id = open.slice(i + 1);
      const row = await api.meeting(token, deviceId, id, pushId);
      return { entry: row ? toEntry(deviceId, id, row) : null };
    },
    onRejected,
    [api, token, open, pushId],
  );
  const entry = bodyLoad.data?.entry ?? rows.find((r) => r.key === open) ?? null;
  const body: BodyState = bodyLoad.error
    ? { status: "error", message: bodyLoad.error, onRetry: bodyLoad.retry }
    : !bodyLoad.data
      ? { status: "loading" }
      : bodyLoad.data.entry
        ? { status: "ready" }
        : { status: "gone" };

  const meta = (startedAt: number, endedAt: number | null) => {
    const min = duration({ started_at: startedAt, endedAt });
    return `${clock(startedAt)}${min ? ` · ${min}` : ""}`;
  };
  const indexOf = (id: string) =>
    Math.max(0, devices?.findIndex((d) => d.id === id) ?? 0);
  const nameOf = (id: string) =>
    devices?.find((d) => d.id === id)?.name ?? "Unknown device";

  const openDevice = open ? open.slice(0, open.indexOf("/")) : "";
  // Chat asks about the live library only, never as of an earlier push.
  const chat = useChat(api, token, open && !pushId ? { device_id: openDevice, meeting_id: open.slice(open.indexOf("/") + 1) } : null, onRejected);
  // The browser's zone, so "today" and "in December" mean the asker's days. It is read once, so a zone change mid-chat does not start it over.
  const [tz] = useState(() => Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC");
  const libraryChat = useChat(api, token, pushId ? null : { tz }, onRejected);
  // Opening a meeting folds the library chat, so coming back shows the pill rather than a window that grabs focus.
  const setLibraryChatOpen = libraryChat?.setOpen;
  useEffect(() => {
    if (open) setLibraryChatOpen?.(false);
  }, [open, setLibraryChatOpen]);
  const cite = (id: string, className: string, children: ReactNode) => {
    const to = { ...loc, meeting: id };
    return (
      <a
        href={format(to)}
        className={className}
        onClick={(e) => {
          if (e.metaKey || e.ctrlKey || e.shiftKey) return;
          e.preventDefault();
          go(to);
        }}
      >
        {children}
      </a>
    );
  };
  const ask = libraryChat && (
    <Ask chat={libraryChat} copy={LIBRARY_CHAT_COPY} renderExtra={(m) => <ChatSources message={m} link={cite} />} position={`${DOCKED} mt-auto`} />
  );
  const meeting = open && (
    <MeetingView
      key={open}
      entry={entry}
      device={nameOf(openDevice)}
      deviceIndex={indexOf(openDevice)}
      back={archived ? "Archive" : starred ? "Starred" : (label?.name ?? devices?.find((d) => d.id === device)?.name ?? "All devices")}
      onBack={() => back({ ...loc, meeting: null })}
      desktop={desktop}
      body={body}
      chat={chat}
    />
  );
  if (!desktop && meeting)
    return (
      <main className="min-h-dvh flex flex-col px-[16px] pt-[16px] pb-[16px]">
        {meeting}
      </main>
    );

  // Another Mac's star and labels mean nothing on this one, so a new Mac starts at All.
  const chooseDevice = (id: string | null) => {
    setDevice(id);
    setFilter("all");
    if (page > 1) go({ ...loc, page: 1 });
  };
  const chooseFilter = (f: Filter) => {
    setFilter(f);
    if (page > 1) go({ ...loc, page: 1 });
  };
  const failed = base.error ?? pageLoad.error;
  const retry = () => (base.error ? base : pageLoad).retry();

  const dim = search.active ? search.pending : turning;
  const list = (
    <>
      {pushId && (
        <div className="flex items-center gap-[7px] pl-[10px] pr-[4px] py-[4px] bg-card border border-line rounded-pill shadow-[0_8px_18px_-6px_var(--shadow-card-color)] text-[13px] min-[721px]:text-[11px] min-w-0">
          <span className="w-[6px] h-[6px] rounded-full bg-blue flex-none" />
          <span className="flex-1 min-w-0 truncate">
            <span className="text-mut">Viewing </span>
            <b className="font-semibold">
              {base.data?.pushedAt ? asOfLabel(base.data.pushedAt) : `push ${pushId}`}
            </b>
          </span>
          <button
            type="button"
            onClick={() => go(HOME)}
            className="flex-none bg-blue text-white font-semibold text-[13px] min-[721px]:text-[10.5px] leading-normal px-[9px] py-[4px] rounded-pill border-0 cursor-pointer font-[inherit]"
          >
            Latest
          </button>
        </div>
      )}
      {!pushId && (
        <SearchField value={query} onChange={setQuery} inputRef={field} />
      )}
      {!!devices?.length && (
        <DevicePills devices={devices} value={device} onChange={chooseDevice} />
      )}
      <Pills label="Filter" options={device ? DEVICE_FILTERS : FILTERS} value={typeof filter === "string" ? filter : null} onChange={chooseFilter}>
        {device && <LabelMenu labels={labels.data} error={labels.error} value={label} onChange={chooseFilter} />}
      </Pills>
      {!pushId && (
        <p
          role="status"
          className="text-[15px] min-[721px]:text-[12px] text-mut m-[0px] break-words [overflow-wrap:anywhere] empty:hidden"
        >
          {searchStatus(search, query)}
        </p>
      )}
      {search.error ? (
        <LoadError
          message={`Search failed. ${search.error}`}
          onRetry={() => {
            search.retry();
            field.current?.focus();
          }}
        />
      ) : (
        <div
          aria-busy={dim || undefined}
          inert={dim || undefined}
          className={`flex flex-col gap-[10px] min-[721px]:gap-[8px] ${dim ? "opacity-50" : ""}`}
        >
          <DayList
            groups={groups}
            render={(r) => (
              <ListRow
                key={r.key}
                title={r.title}
                meta={meta(r.started_at, r.endedAt)}
                tag={
                  // With one Mac picked every row would show the same tag.
                  !device && (
                    <DeviceTag
                      name={nameOf(r.device_id)}
                      index={indexOf(r.device_id)}
                    />
                  )
                }
                snippet={r.snippet}
                selected={r.key === open}
                onClick={() => go({ ...loc, meeting: r.key })}
              />
            )}
          />
          {!search.active && shown && !rows.length && (
            <p className="text-[15px] min-[721px]:text-[12px] text-mut m-[0px]">
              {!shown.archived
                ? "No meetings here."
                : pushId
                  ? "No archived meetings at this push."
                  : "No archived meetings. A meeting you archive on your Mac shows here after its next push."}
            </p>
          )}
        </div>
      )}
      {pageLoad.error && shown ? (
        <LoadError
          message={`Could not load this page. ${pageLoad.error}`}
          onRetry={pageLoad.retry}
        />
      ) : search.active ? (
        (search.offset > 0 || search.more) &&
        !search.error && (
          <SearchPager
            canPrevious={search.offset > 0 && !search.pending}
            canNext={search.more && !search.pending}
            onPrevious={() => search.setPage(search.offset / SEARCH_PAGE - 1)}
            onNext={() => search.setPage(search.offset / SEARCH_PAGE + 1)}
          />
        )
      ) : (
        !!shown?.total && (
          <Pager
            page={page}
            size={size}
            total={shown.total}
            onPage={(next) => {
              go({ ...loc, page: next });
              window.scrollTo(0, 0);
            }}
            onSize={(next) => {
              rememberPageSize(next);
              go({ ...loc, size: next, page: 1 });
            }}
          />
        )
      )}
    </>
  );

  return (
    <div className="min-h-dvh flex flex-col">
      <header className="flex items-center justify-between gap-[10px] px-[16px] py-[16px] min-[721px]:px-[20px] min-[721px]:py-[12px] min-[721px]:bg-card min-[721px]:border-b min-[721px]:border-line">
        {/* The way back to the library from a meeting on a desktop, which has no Back link. */}
        <a
          href={format({ ...loc, meeting: null }) || "#"}
          aria-label="Library"
          className="no-underline text-inherit"
          onClick={(e) => {
            e.preventDefault();
            go({ ...loc, meeting: null });
          }}
        >
          <Brand />
        </a>
        <div className="flex items-center gap-[6px] min-[721px]:gap-[8px]">
          <span className="text-[12px] min-[721px]:text-[11px] font-bold px-[10px] py-[3px] rounded-pill bg-slatesoft text-slate">
            read-only
          </span>
          <button
            type="button"
            aria-label="Settings"
            onClick={onSettings}
            className="w-[24px] h-[24px] min-[721px]:w-[30px] min-[721px]:h-[30px] grid place-items-center rounded-full border border-line bg-card text-mut cursor-pointer"
          >
            <GearIcon />
          </button>
          {desktop && <LockButton onClick={onLock} />}
        </div>
      </header>
      {failed && !(devices && shown) ? (
        <main className="flex-1 flex flex-col items-center px-[16px] pt-[80px]">
          <LoadError
            message={`Could not load your library. ${failed}`}
            onRetry={retry}
          />
        </main>
      ) : !devices || !shown ? null : pageLoad.data?.empty ? (
        <main className="flex-1 flex flex-col items-center text-center px-[16px] pt-[80px]">
          <div className="float-3500 mb-[32px]">
            <Logo size={64} ariaLabel="Yogurt" />
          </div>
          <h2 className="heading-state mb-[12px]">
            {devices.length
              ? "Waiting for the first push"
              : "No machine has pushed yet"}
          </h2>
          <p className="text-[15px] text-mut max-w-md m-0">
            {devices.length
              ? `${devices.length} ${devices.length === 1 ? "machine is" : "machines are"} connected. Meetings show here after a push.`
              : "Connect a laptop in yogurt's Settings under Cloud sync, then push your meetings. They show up here."}
          </p>
        </main>
      ) : desktop ? (
        <div className="flex-1 grid grid-cols-[300px_minmax(0,1fr)]">
          <aside className="border-r border-line p-[14px] flex flex-col gap-[8px]">
            {list}
          </aside>
          <main className="px-[28px] py-[22px] flex flex-col">
            {meeting ?? (
              <>
                <p className="text-[13px] text-mut m-[0px]">
                  Pick a meeting to read it.
                </p>
                {ask}
              </>
            )}
          </main>
        </div>
      ) : (
        <main className="flex-1 flex flex-col gap-[10px] px-[16px] pb-[16px]">
          {list}
          {ask}
        </main>
      )}
    </div>
  );
}
