import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { Upload } from "lucide-react";
import { Button } from "../Button";
import { useImportMeetingFile } from "../../lib/api/meetings";

/** Imports `.yogurt` files someone exported from their yogurt, and opens the meeting when there is one. */
export function ImportMeetingButton() {
  const input = useRef<HTMLInputElement>(null);
  const navigate = useNavigate();
  const imp = useImportMeetingFile();
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string[] | null>(null);

  useEffect(() => {
    if (!message) return;
    const t = setTimeout(() => setMessage(null), 6000);
    return () => clearTimeout(t);
  }, [message]);

  const onChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? []);
    // Cleared so picking the same files again still fires `change`.
    e.target.value = "";
    if (files.length === 0) return;
    setBusy(true);
    setMessage(null);
    const ids: string[] = [];
    const failed: string[] = [];
    for (const file of files) {
      try {
        ids.push(await imp.mutateAsync(file));
      } catch (err) {
        const reason = (err as Error).message;
        failed.push(files.length === 1 ? reason : `${file.name}: ${reason}`);
      }
    }
    setBusy(false);
    // The ref is null once the user has left the Library, so a late upload does not pull them back.
    if (files.length === 1 && ids.length === 1) {
      if (input.current) navigate(`/meeting/${ids[0]}/post`);
    } else if (files.length === 1) setMessage([`Couldn't import the meeting: ${failed[0]}`]);
    else if (failed.length) setMessage([`Couldn't import ${failed.length} of ${files.length} files:`, ...failed]);
    else setMessage([`Imported ${ids.length} meetings`]);
  };

  return (
    <div className="relative">
      <Button
        variant="secondary"
        size="sm"
        disabled={busy}
        onClick={() => input.current?.click()}
      >
        <Upload size={14} aria-hidden />
        {busy ? "Importing…" : "Import"}
      </Button>
      <input
        ref={input}
        type="file"
        accept=".yogurt,application/json"
        multiple
        aria-label="Import meeting files"
        className="hidden"
        onChange={onChange}
      />
      {message && (
        <p
          role="alert"
          className="absolute top-full right-0 mt-1 z-20 w-64 max-h-64 overflow-y-auto px-3 py-2 bg-card border border-line rounded-card shadow-pop text-[12px] text-ink"
        >
          {message.map((line, i) => (
            <span key={i} className="block break-words">
              {line}
            </span>
          ))}
        </p>
      )}
    </div>
  );
}
