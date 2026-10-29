import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent, act, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { DONE_PILL_MS } from "../hooks/hideFinishedPill";
import { MemoryRouter } from "react-router";

const navigateSpy = vi.hoisted(() => vi.fn());
vi.mock("react-router", async () => {
  const actual = await vi.importActual<typeof import("react-router")>("react-router");
  return { ...actual, useNavigate: () => navigateSpy };
});

import { ImportPill } from "./ImportPill";
import {
  applyImportEvent,
  clearImport,
  type ImportEvent,
} from "../hooks/useGranolaImport";

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
  vi.useRealTimers();
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
    const open = within(pill).getAllByRole("button")[0];
    expect(pill).toHaveTextContent("Importing from Granola · 340 of 1,365");
    fireEvent.click(open);
    expect(navigateSpy).toHaveBeenCalledWith("/settings/import");
    expect(screen.getByTestId("import-pill")).toBeInTheDocument();
  });

  it("reads Imported on completion, opens the Library, then goes away", () => {
    renderPill();
    send({ type: "import_complete", imported: 1365, skipped: 0, failed: 0, failed_titles: [] });
    const pill = screen.getByTestId("import-pill");
    const open = within(pill).getAllByRole("button")[0];
    expect(pill).toHaveTextContent("Imported 1,365");
    fireEvent.click(open);
    expect(navigateSpy).toHaveBeenCalledWith("/");
    expect(screen.queryByTestId("import-pill")).not.toBeInTheDocument();
  });

  it("goes away on its own a few seconds after the import finishes", () => {
    vi.useFakeTimers();
    renderPill();
    send({ type: "import_complete", imported: 5, skipped: 0, failed: 0, failed_titles: [] });
    act(() => vi.advanceTimersByTime(DONE_PILL_MS - 1));
    expect(screen.getByTestId("import-pill")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(1));
    expect(screen.queryByTestId("import-pill")).not.toBeInTheDocument();
  });

  it("keeps a new import's progress when the previous one's timer fires", () => {
    vi.useFakeTimers();
    renderPill();
    send({ type: "import_complete", imported: 5, skipped: 0, failed: 0, failed_titles: [] });
    send(progress(1));
    act(() => vi.advanceTimersByTime(DONE_PILL_MS));
    expect(screen.getByTestId("import-pill")).toHaveTextContent("Importing from Granola · 1 of 1,365");
  });

  it("closes a finished import from its close button without leaving the page", () => {
    renderPill();
    send(progress(10));
    expect(screen.queryByRole("button", { name: "Close the Granola import notice" })).not.toBeInTheDocument();
    send({ type: "import_complete", imported: 5, skipped: 0, failed: 0, failed_titles: [] });
    fireEvent.click(screen.getByRole("button", { name: "Close the Granola import notice" }));
    expect(screen.queryByTestId("import-pill")).not.toBeInTheDocument();
    expect(navigateSpy).not.toHaveBeenCalled();
  });

  it("names failed meetings and stays up when some of the import failed", () => {
    vi.useFakeTimers();
    renderPill();
    send({ type: "import_complete", imported: 1300, skipped: 0, failed: 65, failed_titles: ["A"] });
    act(() => vi.advanceTimersByTime(60_000));
    expect(screen.getByTestId("import-pill")).toHaveTextContent("Imported 1,300 · 65 failed");
  });

  it("starts the countdown only once a background tab is visible again", () => {
    vi.useFakeTimers();
    const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
    renderPill();
    send({ type: "import_complete", imported: 5, skipped: 0, failed: 0, failed_titles: [] });
    act(() => vi.advanceTimersByTime(60_000));
    expect(screen.getByTestId("import-pill")).toBeInTheDocument();
    hidden.mockReturnValue(false);
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    act(() => vi.advanceTimersByTime(DONE_PILL_MS));
    expect(screen.queryByTestId("import-pill")).not.toBeInTheDocument();
    hidden.mockRestore();
  });

  it("keeps a failure up until it is closed", () => {
    vi.useFakeTimers();
    renderPill();
    send({ type: "import_error", error: "Granola is unreachable." });
    act(() => vi.advanceTimersByTime(60_000));
    fireEvent.click(screen.getByRole("button", { name: "Close the Granola import notice" }));
    expect(screen.queryByTestId("import-pill")).not.toBeInTheDocument();
  });

  it("shows a failure in the error tone and opens Settings, Import", () => {
    renderPill();
    send(progress(10));
    send({ type: "import_error", error: "Granola is unreachable." });
    const open = screen.getByRole("button", { name: /show granola import error/i });
    const pill = screen.getByTestId("import-pill");
    expect(pill).toHaveTextContent("Import failed");
    expect(pill).toHaveClass("border-straw");
    fireEvent.click(open);
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
