import { Link } from "react-router";
import { ArrowLeft } from "lucide-react";
import type { ProviderView } from "../../lib/api/settings";

/**
 * Settings page left rail — Phase 5 (Plan 05-03), SET-01..SET-03.
 *
 * Width: load-bearing `212px` (PRD §5.6 / §16.8 / 05-UI-SPEC.md). This is
 * the same width the Phase 7 library sidebar will use, but the two
 * components are deliberately NOT shared — the section list differs.
 *
 * Footer:
 * - Green matcha "Local-only · on" pill rendered iff no active provider
 *   has a non-localhost base URL (PRD §5.6 D-14).
 * - JetBrains-Mono caption disclosing where keys and data live. The
 *   `→` glyph is U+2192 (never `->` ASCII — UI-SPEC §Copywriting).
 */

export type SettingsSection = "model" | "transcription" | "audio" | "general";

interface Props {
  active: SettingsSection;
  onChange: (s: SettingsSection) => void;
  providers: ProviderView[];
}

const SECTIONS: { id: SettingsSection; label: string }[] = [
  { id: "model", label: "Model" },
  { id: "transcription", label: "Transcription" },
  { id: "audio", label: "Audio" },
  { id: "general", label: "General" },
];

export function SidebarNav({ active, onChange, providers }: Props) {
  // "Local-only" is true iff no active provider points to a non-localhost
  // host. Matches the server-side privacy posture (UI-SPEC §Interaction 1).
  const localOnly = !providers.some(
    (p) => p.is_active && !/localhost|127\.0\.0\.1/.test(p.base_url),
  );

  return (
    <nav
      className="w-[212px] shrink-0 bg-[var(--color-paper)] border-r border-line flex flex-col"
      aria-label="Settings sections"
    >
      <header className="px-5 pt-6 pb-2 space-y-3">
        <Link
          to="/"
          className="inline-flex items-center gap-1.5 text-[12px] font-mono uppercase tracking-wider text-mut hover:text-ink transition-colors"
          aria-label="Back to library"
        >
          <ArrowLeft size={16} aria-hidden />
          <span>Library</span>
        </Link>
        <h1 className="font-serif text-[22px] leading-none text-ink">
          Settings
        </h1>
      </header>
      <ul className="flex-1 py-4 px-3 space-y-1">
        {SECTIONS.map((s) => (
          <li key={s.id}>
            <button
              type="button"
              onClick={() => onChange(s.id)}
              className={`w-full text-left px-3 py-2 rounded-md text-[13.5px] ${
                active === s.id
                  ? "bg-[var(--color-blsoft)] text-[var(--color-blue)] font-semibold"
                  : "text-ink hover:bg-line/40 font-medium"
              }`}
            >
              {s.label}
            </button>
          </li>
        ))}
      </ul>
      <footer className="p-4 border-t border-line space-y-2">
        {localOnly ? (
          <span
            className="inline-flex items-center gap-1.5 text-xs font-semibold text-white bg-[var(--color-matcha)] px-2.5 py-1 rounded-full"
            data-testid="local-only-pill"
          >
            <span className="w-1.5 h-1.5 rounded-full bg-white" />
            Local-only · on
          </span>
        ) : null}
        <div className="font-mono text-[10.5px] text-mut leading-relaxed">
          keys → macOS Keychain
          <br />
          data → ~/.yogurt/
        </div>
      </footer>
    </nav>
  );
}
