import { Fragment } from "react";
import { Link } from "react-router";
import { ArrowLeft } from "lucide-react";
import type { DataPaths, ProviderView } from "../../lib/api/settings";
import { Pill } from "../Pill";
import { useCloudStatus } from "../../lib/api/cloud";

// The 212px width matches the Library sidebar, but the components are deliberately
// not shared because the section lists differ.
// The `→` in the caption is U+2192, never ASCII `->`.

export type SettingsSection = "model" | "transcription" | "audio" | "general" | "cloud" | "import";

interface Props {
  active: SettingsSection;
  onChange: (s: SettingsSection) => void;
  providers: ProviderView[];
  paths: DataPaths;
}

const SECTIONS: { id: SettingsSection; label: string }[] = [
  { id: "model", label: "Model" },
  { id: "transcription", label: "Transcription" },
  { id: "audio", label: "Audio" },
  { id: "general", label: "General" },
  { id: "cloud", label: "Cloud sync" },
  { id: "import", label: "Import" },
];

export function SidebarNav({ active, onChange, providers, paths }: Props) {
  // Local-only holds iff no active provider points to a non-localhost host.
  const localOnly = !providers.some(
    (p) => p.is_active && !/localhost|127\.0\.0\.1/.test(p.base_url),
  );

  const cloud = useCloudStatus().data;
  const cloudNew = cloud?.connected === false;
  const cloudOn = cloud?.connected === true;

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
              {s.id === "cloud" && cloudNew && (
                <Pill
                  tone="blue"
                  className={`ml-2 font-bold! ${active === s.id ? "bg-card!" : ""}`}
                >
                  new
                </Pill>
              )}
            </button>
          </li>
        ))}
      </ul>
      <footer className="p-4 border-t border-line space-y-2">
        {cloudOn ? (
          <span data-testid="cloud-sync-pill" className="inline-block">
            <Pill tone="matcha" className="text-ink!">
              <span className="w-1.5 h-1.5 rounded-pill bg-matcha" />
              Cloud sync · on
            </Pill>
          </span>
        ) : localOnly ? (
          <span data-testid="local-only-pill" className="inline-block">
            <Pill tone="matcha" className="text-ink!">
              <span className="w-1.5 h-1.5 rounded-pill bg-matcha" />
              Local-only · on
            </Pill>
          </span>
        ) : null}
        <div className="text-[13px] text-mut leading-relaxed">
          keys → <PathCode path={paths.keys} />
          <br />
          data → <PathCode path={`${paths.data_dir}/`} />
        </div>
      </footer>
    </nav>
  );
}

/** Wraps a long path after a `/` rather than mid-name. `<wbr>` keeps a copied path clean. */
function PathCode({ path }: { path: string }) {
  return (
    <code className="font-mono [overflow-wrap:anywhere]">
      {path.split("/").map((seg, i) => (
        <Fragment key={i}>
          {i > 0 && (
            <>
              /<wbr />
            </>
          )}
          {seg}
        </Fragment>
      ))}
    </code>
  );
}
