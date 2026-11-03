import { useQuery } from "@tanstack/react-query";
import { fetchHealth } from "../../lib/api";
import { Pill } from "../Pill";
import { LABEL } from "./labelClass";

const REPO = "https://github.com/jarvisrchen/yogurt";
const LINK = "text-[13.5px] font-medium text-blue hover:underline";

export function AboutSection() {
  const health = useQuery({
    queryKey: ["health"],
    queryFn: fetchHealth,
    staleTime: Infinity,
    retry: false,
  });
  const version = health.data?.version;

  return (
    <section className="space-y-4">
      <header className="space-y-1">
        <h2 className="heading-section">About</h2>
        <p className="text-[13px] text-mut">
          A local-first meeting copilot for macOS. Yogurt records your mic and
          system audio without a meeting bot, transcribes it live, and fills in
          your notes from the transcript. Zero telemetry.
        </p>
      </header>

      <dl className="rounded-card border border-line bg-card p-5 grid grid-cols-[120px_minmax(0,1fr)] gap-x-4 gap-y-3 items-center">
        <dt className={LABEL}>Version</dt>
        <dd className="flex items-center gap-2 text-[13.5px] text-ink">
          {health.isError ? (
            <span className="text-mut">Unavailable</span>
          ) : version ? (
            <>
              <code data-testid="about-version" className="font-mono">
                v{version}
              </code>
              {health.data?.mode === "dev" && <Pill>dev build</Pill>}
            </>
          ) : (
            <span className="h-4 w-16 rounded shimmer" role="status" aria-label="Loading version" />
          )}
        </dd>

        <dt className={LABEL}>License</dt>
        <dd className="text-[13.5px] text-ink">MIT</dd>

        <dt className={LABEL}>Source</dt>
        <dd className="flex flex-wrap gap-x-4 gap-y-1">
          <a href={REPO} target="_blank" rel="noopener noreferrer" className={LINK}>
            GitHub
          </a>
          {/* A dev build's version has no tag until the release is pushed. */}
          {version && health.data?.mode === "release" && (
            <a
              href={`${REPO}/releases/tag/v${version}`}
              target="_blank"
              rel="noopener noreferrer"
              className={LINK}
            >
              Release notes
            </a>
          )}
        </dd>
      </dl>
    </section>
  );
}
