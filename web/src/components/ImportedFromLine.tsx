import type { MeetingSourceJson } from "../lib/api/meetings";

export function ImportedFromLine({ source }: { source: MeetingSourceJson }) {
  const owner = source.owner;
  const capturedBy =
    owner && owner.is_user !== true ? owner.name || owner.email : null;
  return (
    <p data-testid="imported-from" className="text-[12px] text-mut">
      Imported from Granola
      {capturedBy && <> · Captured by {capturedBy}</>}
    </p>
  );
}
