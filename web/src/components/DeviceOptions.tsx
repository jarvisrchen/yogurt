import type { AudioDevice } from "../lib/api/settings";

/** `<option>`s for a device select; a `selected` name missing from `devices`
 *  (unplugged) is still listed so the select shows the real setting. */
export function DeviceOptions({
  devices,
  selected,
}: {
  devices: AudioDevice[] | undefined;
  selected: string;
}) {
  const missing = selected && !devices?.some((d) => d.name === selected);
  return (
    <>
      {devices?.map((d) => (
        <option key={d.name} value={d.name}>
          {d.name}
          {d.is_default ? " (default)" : ""}
        </option>
      ))}
      {missing && <option value={selected}>{selected} (unavailable)</option>}
    </>
  );
}
