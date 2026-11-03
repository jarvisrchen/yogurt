import { useEffect, useRef } from "react";
import { useNavigate } from "react-router";
import { Upload } from "lucide-react";
import { Button } from "../Button";
import { useImportMeetingFile } from "../../lib/api/meetings";

/** Imports a `.yogurt` file someone exported from their yogurt and opens it. */
export function ImportMeetingButton() {
  const input = useRef<HTMLInputElement>(null);
  const navigate = useNavigate();
  const imp = useImportMeetingFile();
  const { error, reset } = imp;

  useEffect(() => {
    if (!error) return;
    const t = setTimeout(reset, 6000);
    return () => clearTimeout(t);
  }, [error, reset]);

  const onChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    // Cleared so picking the same file again still fires `change`.
    e.target.value = "";
    if (!file) return;
    imp.mutate(file, { onSuccess: (id) => navigate(`/meeting/${id}/post`) });
  };

  return (
    <div className="relative">
      <Button
        variant="secondary"
        size="sm"
        disabled={imp.isPending}
        onClick={() => input.current?.click()}
      >
        <Upload size={14} aria-hidden />
        {imp.isPending ? "Importing…" : "Import"}
      </Button>
      <input
        ref={input}
        type="file"
        accept=".yogurt,application/json"
        aria-label="Import a meeting file"
        className="hidden"
        onChange={onChange}
      />
      {error && (
        <p
          role="alert"
          className="absolute top-full right-0 mt-1 z-20 w-64 px-3 py-2 bg-card border border-line rounded-card shadow-pop text-[12px] text-ink"
        >
          Couldn't import the meeting: {error.message}
        </p>
      )}
    </div>
  );
}
