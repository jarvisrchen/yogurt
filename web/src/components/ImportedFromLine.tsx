import type { MeetingSourceJson } from "../lib/api/meetings";

export function ImportedFromLine({ source }: { source: MeetingSourceJson }) {
  const owner = source.owner;
  const capturedBy =
    owner && owner.is_user !== true ? owner.name || owner.email : null;
  const url = source.web_url && /^https?:\/\//i.test(source.web_url) ? source.web_url : null;
  return (
    <p data-testid="imported-from" className="text-[12px] text-mut">
      Imported from Granola
      {capturedBy && <> · Captured by {capturedBy}</>}
      {url && (
        <>
          {" · "}
          <a
            href={url}
            target="_blank"
            rel="noopener noreferrer"
            className="underline underline-offset-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue"
          >
            Open in Granola
          </a>
        </>
      )}
    </p>
  );
}
