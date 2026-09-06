import { Link } from "react-router";
import { ArrowLeft } from "lucide-react";
import type { ProviderView } from "../../lib/api/settings";
import { Pill } from "../Pill";

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
 * - Caption disclosing where keys and data live (paths rendered in mono). The
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
      className="w-[212px] shrink-0 bg-paper border-r border-line flex flex-col"
      aria-label="Settings sections"
    >
      <header className="px-5 pt-6 pb-2 space-y-3">
        <Link
          to="/"
          className="inline-flex items-center gap-1.5 text-[13px] font-medium text-mut hover:text-ink transition-colors"
          aria-label="Back to library"
        >
          <ArrowLeft size={16} aria-hidden />
          <span>Library</span>
        </Link>
        <h1 className="heading-wordmark">
          Settings
        </h1>
      </header>
      <ul className="flex-1 py-4 px-3 space-y-1">
        {SECTIONS.map((s) => (
          <li key={s.id}>
            <button
              type="button"
              onClick={() => onChange(s.id)}
              className={`w-full text-left px-3 py-2 rounded-button text-[13.5px] ${
                active === s.id
                  ? "bg-blsoft text-blue font-semibold"
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
          <span data-testid="local-only-pill" className="inline-block">
            <Pill tone="matcha" className="text-white bg-matcha">
              <span className="w-1.5 h-1.5 rounded-pill bg-white" />
              Local-only · on
            </Pill>
          </span>
        ) : null}
        <div className="text-[13px] text-mut leading-relaxed">
          keys → <code className="font-mono">~/.yogurt/keys.json</code>
          <br />
          data → <code className="font-mono">~/.yogurt/</code>
        </div>
      </footer>
    </nav>
  );
}
