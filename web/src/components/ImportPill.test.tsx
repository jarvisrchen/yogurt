import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent, act } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MemoryRouter } from "react-router";

const navigateSpy = vi.hoisted(() => vi.fn());
vi.mock("react-router", async () => {
  const actual = await vi.importActual<typeof import("react-router")>("react-router");
  return { ...actual, useNavigate: () => navigateSpy };
});

import { ImportPill } from "./ImportPill";
import { applyImportEvent, clearImport, type ImportEvent } from "../hooks/useGranolaImport";

const send = (ev: ImportEvent) => act(() => applyImportEvent(ev));
const progress = (done: number): ImportEvent => ({
  type: "import_progress",
  done,
  total: 1365,
  imported: done,
  skipped: 0,
  failed: 0,
});

afterEach(() => {
  cleanup();
  act(() => clearImport());
  navigateSpy.mockClear();
});

const renderPill = () =>
  render(
    <MemoryRouter>
      <ImportPill />
    </MemoryRouter>,
  );

describe("ImportPill", () => {
  it("renders nothing until an import runs", () => {
    renderPill();
    expect(screen.queryByTestId("import-pill")).not.toBeInTheDocument();
  });

  it("shows progress with thousands separators and opens Settings, Import", () => {
    renderPill();
    send(progress(340));
    const pill = screen.getByTestId("import-pill");
    expect(pill).toHaveTextContent("Importing from Granola · 340 of 1,365");
    fireEvent.click(pill);
    expect(navigateSpy).toHaveBeenCalledWith("/settings/import");
    expect(screen.getByTestId("import-pill")).toBeInTheDocument();
  });

  it("reads Imported on completion, opens the Library, then goes away", () => {
    renderPill();
    send({ type: "import_complete", imported: 1365, skipped: 0, failed: 0, failed_titles: [] });
    const pill = screen.getByTestId("import-pill");
    expect(pill).toHaveTextContent("Imported 1,365");
    fireEvent.click(pill);
    expect(navigateSpy).toHaveBeenCalledWith("/");
    expect(screen.queryByTestId("import-pill")).not.toBeInTheDocument();
  });

  it("stays on a finished import until clicked", () => {
    vi.useFakeTimers();
    renderPill();
    send({ type: "import_complete", imported: 5, skipped: 0, failed: 0, failed_titles: [] });
    act(() => vi.advanceTimersByTime(60_000));
    vi.useRealTimers();
    expect(screen.getByTestId("import-pill")).toBeInTheDocument();
  });

  it("shows a failure in the error tone and opens Settings, Import", () => {
    renderPill();
    send(progress(10));
    send({ type: "import_error", error: "Granola is unreachable." });
    const pill = screen.getByRole("button", { name: /show granola import error/i });
    expect(pill).toHaveTextContent("Import failed");
    expect(pill).toHaveClass("border-straw");
    fireEvent.click(pill);
    expect(navigateSpy).toHaveBeenCalledWith("/settings/import");
    expect(screen.queryByTestId("import-pill")).not.toBeInTheDocument();
  });

  it("disappears when the import is cancelled", () => {
    renderPill();
    send(progress(10));
    send({ type: "import_cancelled", imported: 10, skipped: 0, failed: 0 });
    expect(screen.queryByTestId("import-pill")).not.toBeInTheDocument();
  });
});
