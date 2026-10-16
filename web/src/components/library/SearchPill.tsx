import { useEffect, useState } from "react";
import { Search } from "lucide-react";

interface SearchPillProps {
  value: string;
  onChange: (next: string) => void;
}

export function SearchPill({ value, onChange }: SearchPillProps) {
  const [local, setLocal] = useState(value);

  // Follow parent-forced changes such as clearing on route change.
  useEffect(() => {
    setLocal(value);
  }, [value]);

  // Collapse a burst of keystrokes into one server query.
  useEffect(() => {
    if (local === value) return;
    const id = setTimeout(() => onChange(local), 200);
    return () => clearTimeout(id);
  }, [local, value, onChange]);

  return (
    <div
      role="search"
      className="flex items-center gap-2 bg-card border border-line rounded-pill px-4 py-2 text-[13px] text-ink w-[280px] focus-within:ring-2 focus-within:ring-blue/40"
    >
      <Search size={16} className="text-mut shrink-0" aria-hidden />
      <input
        type="text"
        value={local}
        onChange={(e) => setLocal(e.target.value)}
        placeholder="Search notes & transcripts"
        aria-label="Search notes and transcripts"
        className="flex-1 bg-transparent outline-none placeholder:text-mut"
      />
    </div>
  );
}
