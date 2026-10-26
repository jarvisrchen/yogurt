import { useMemo, useState } from "react";
import { groupMeetingsByDay } from "@web/components/library/dayGroups";
import { Back, DayList, DevicePills, DeviceTag, ListRow, LoadError, LockButton, useDesktop } from "../ui";
import type { Api, HistoryRow } from "../lib/api";
import { clock, forDevice, historyLabel } from "../lib/library";
import { useCloudLoad } from "../lib/useCloudLoad";

export function History({ api, token, onBack, onOpen, onLock, onRejected }: { api: Api; token: string; onBack: () => void; onOpen: (row: HistoryRow) => void; onLock: () => void; onRejected: () => void }) {
  const desktop = useDesktop();
  const [device, setDevice] = useState<string | null>(null);
  const { data, error, retry } = useCloudLoad(async () => {
    const [devices, rows] = await Promise.all([api.devices(token), api.history(token)]);
    return { devices, rows };
  }, onRejected, [api, token]);
  const { devices, rows } = data ?? {};

  const groups = useMemo(() => groupMeetingsByDay(forDevice(rows?.map((r) => ({ ...r, started_at: Date.parse(r.pushed_at) })) ?? [], device)), [rows, device]);

  return (
    <div className="min-h-dvh flex flex-col">
      {desktop && (
        <div className="flex items-center justify-between gap-[10px] px-[20px] py-[12px] bg-card border-b border-line">
          <Back to="Settings" onClick={onBack} />
          <LockButton onClick={onLock} />
        </div>
      )}
      <main className="w-full max-w-[640px] mx-auto flex flex-col gap-[10px] px-[16px] pt-[16px] pb-[16px] min-[721px]:px-[24px] min-[721px]:pt-[22px] min-[721px]:pb-[28px]">
        {!desktop && <Back to="Settings" onClick={onBack} />}
        <div>
          <h1 className="text-[16px] min-[721px]:text-[22px] font-extrabold tracking-[-0.01em] text-ink m-[0px]">History</h1>
          <p className="text-[10.5px] min-[721px]:text-[12.5px] text-mut mt-[3px] mb-[0px]">Every push is kept. Tap one to see your library as it was then.</p>
        </div>
        {error ? (
          <LoadError message={`Could not load History. ${error}`} onRetry={retry} />
        ) : (
          devices &&
          rows && (
            <>
              <DevicePills devices={devices} value={device} onChange={setDevice} />
              <DayList
                groups={groups}
                render={(r) => (
                  <ListRow
                    key={r.push_id}
                    title={historyLabel(r.changed, r.deleted)}
                    meta={clock(r.started_at)}
                    tag={<DeviceTag name={r.device_name} index={Math.max(0, devices.findIndex((d) => d.id === r.device_id))} />}
                    onClick={() => onOpen(r)}
                  />
                )}
              />
              {!groups.length && <p className="text-[12px] text-mut m-[0px]">No pushes yet.</p>}
            </>
          )
        )}
      </main>
    </div>
  );
}
