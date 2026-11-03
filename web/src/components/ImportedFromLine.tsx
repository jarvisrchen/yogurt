import type { MeetingSourceJson } from "../lib/api/meetings";

const FROM: Record<string, string> = { granola: "Granola", yogurt: "a yogurt file" };

export const importedFrom = (name: string) => FROM[name] ?? name;

export function ImportedFromLine({ name, source }: { name: string; source: MeetingSourceJson }) {
  if (name === "phone") {
    return (
      <p data-testid="imported-from" className="text-[12px] text-mut">
        Recorded on {source.device || "a phone"}
      </p>
    );
  }
  const owner = source.owner;
  const capturedBy =
    owner && owner.is_user !== true ? owner.name || owner.email : null;
  return (
    <p data-testid="imported-from" className="text-[12px] text-mut">
      Imported from {importedFrom(name)}
      {capturedBy && <> · Captured by {capturedBy}</>}
    </p>
  );
}
