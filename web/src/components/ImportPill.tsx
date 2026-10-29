import { useNavigate } from "react-router";
import { hideImportPill, useGranolaImport } from "../hooks/useGranolaImport";
import { TaskPill } from "./TaskPill";

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
            state.failed > 0
              ? `Imported ${n(state.imported)} · ${n(state.failed)} failed`
              : `Imported ${n(state.imported)}`,
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
  const pct =
    state.kind === "running" && state.progress.total > 0
      ? Math.min(100, Math.round((state.progress.done / state.progress.total) * 100))
      : null;

  return (
    <TaskPill
      kind={state.kind}
      pct={pct}
      text={text}
      label={label}
      onClick={onClick}
      closeLabel="Close the Granola import notice"
      onClose={hideImportPill}
      testId="import-pill"
    />
  );
}
