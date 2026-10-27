import clsx from "clsx";
import { useNavigate } from "react-router";
import { hideImportPill, useGranolaImport } from "../hooks/useGranolaImport";
import { ProgressRing } from "./ProgressRing";

const n = (v: number) => v.toLocaleString("en-US");

export function ImportPill() {
  const imp = useGranolaImport();
  const navigate = useNavigate();
  if (!imp || imp.pillHidden || imp.state.kind === "cancelled") return null;
  const { state } = imp;

  const [label, text, onClick] =
    state.kind === "running"
      ? [
          "Show Granola import progress",
          state.progress.total > 0
            ? `Importing from Granola · ${n(state.progress.done)} of ${n(state.progress.total)}`
            : "Importing from Granola",
          () => navigate("/settings/import"),
        ]
      : state.kind === "done"
        ? [
            `Imported ${n(state.imported)} meetings, open the library`,
            `Imported ${n(state.imported)}`,
            () => {
              hideImportPill();
              navigate("/");
            },
          ]
        : [
            "Show Granola import error",
            "Import failed",
            () => {
              hideImportPill();
              navigate("/settings/import");
            },
          ];
  const failed = state.kind === "failed";
  const pct =
    state.kind === "running" && state.progress.total > 0
      ? Math.min(100, Math.round((state.progress.done / state.progress.total) * 100))
      : null;

  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      data-testid="import-pill"
      className={clsx(
        "flex items-center gap-2 px-3 py-1.5 bg-card border rounded-full shadow-pop text-[13px] text-ink whitespace-nowrap transition-colors",
        failed ? "border-straw" : "border-line hover:border-matcha",
      )}
    >
      {state.kind === "running" ? (
        <ProgressRing pct={pct} />
      ) : (
        <span
          aria-hidden="true"
          className={clsx(
            "inline-flex w-3.5 justify-center font-semibold",
            failed ? "text-straw" : "text-matcha",
          )}
        >
          {failed ? "!" : "✓"}
        </span>
      )}
      <span className={clsx("font-medium", failed && "text-straw")}>{text}</span>
    </button>
  );
}
