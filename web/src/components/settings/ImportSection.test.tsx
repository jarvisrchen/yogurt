import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import type { ImportStatus } from "../../lib/api/granolaImport";

const api = vi.hoisted(() => ({
  status: vi.fn(),
  setKey: vi.fn(),
  deleteKey: vi.fn(),
  preview: vi.fn(),
  start: vi.fn(),
  cancel: vi.fn(),
}));
vi.mock("../../lib/api/granolaImport", async () => {
  const actual = await vi.importActual<typeof import("../../lib/api/granolaImport")>("../../lib/api/granolaImport");
  return { ...actual, importApi: api };
});

import { ImportSection } from "./ImportSection";
import { applyImportEvent, clearImport } from "../../hooks/useGranolaImport";

const idle = (key: string | null): ImportStatus => ({ key, running: false, progress: null, last: null });

function renderSection() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <ImportSection />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  Object.values(api).forEach((f) => f.mockReset());
});
afterEach(() => {
  cleanup();
  act(() => clearImport());
});

describe("ImportSection", () => {
  it("with no key, saves a pasted key and clears the field", async () => {
    api.status.mockResolvedValue(idle(null));
    api.setKey.mockImplementation(async () => {
      api.status.mockResolvedValue(idle("••••wxyz"));
      return { key: "••••wxyz" };
    });
    renderSection();
    const input = await screen.findByLabelText("Granola API key");
    expect(input).toHaveAttribute("type", "password");
    const help = screen.getByRole("link", { name: /Granola API help/ });
    expect(help).toHaveAttribute("href", "https://docs.granola.ai/help-center/sharing/integrations/granola-api");
    expect(help).toHaveAttribute("target", "_blank");
    expect(help).toHaveAttribute("rel", "noopener noreferrer");
    fireEvent.change(input, { target: { value: "grn_secret_wxyz" } });
    fireEvent.click(screen.getByRole("button", { name: "Save key" }));

    expect(await screen.findByText("••••wxyz")).toBeInTheDocument();
    expect(api.setKey.mock.calls[0][0]).toBe("grn_secret_wxyz");
    expect(document.body.textContent).not.toContain("grn_secret");
  });

  it("shows the server's message when a key is rejected", async () => {
    api.status.mockResolvedValue(idle(null));
    api.setKey.mockRejectedValue(new Error("Granola rejected the API key."));
    renderSection();
    fireEvent.change(await screen.findByLabelText("Granola API key"), { target: { value: "bad" } });
    fireEvent.click(screen.getByRole("button", { name: "Save key" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Granola rejected the API key.");
  });

  it("with a key, previews the counts and starts the import", async () => {
    api.status.mockResolvedValue(idle("••••wxyz"));
    api.preview.mockResolvedValue({ total: 1365, already_imported: 0, to_import: 1365, captured_by_others: 73 });
    api.start.mockResolvedValue(undefined);
    renderSection();
    expect(await screen.findByText("••••wxyz")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Preview" }));
    expect(await screen.findByTestId("import-preview")).toHaveTextContent(
      "1,365 meetings in Granola · 0 already imported · 73 captured by someone else",
    );
    fireEvent.click(screen.getByRole("button", { name: "Import 1,365 meetings" }));
    await waitFor(() => expect(api.start).toHaveBeenCalled());
    expect(await screen.findByRole("button", { name: "Cancel" })).toBeInTheDocument();
  });

  it("removes the key", async () => {
    api.status.mockResolvedValue(idle("••••wxyz"));
    api.deleteKey.mockImplementation(async () => {
      api.status.mockResolvedValue(idle(null));
    });
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: "Remove" }));
    expect(await screen.findByLabelText("Granola API key")).toBeInTheDocument();
  });

  it("shows progress and counts while importing, and cancels", async () => {
    api.status.mockResolvedValue(idle("••••wxyz"));
    api.cancel.mockResolvedValue(undefined);
    renderSection();
    await screen.findByText("••••wxyz");
    act(() =>
      applyImportEvent({ type: "import_progress", done: 340, total: 1365, imported: 300, skipped: 38, failed: 2 }),
    );
    expect(screen.getByText("340 of 1,365")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "340");
    expect(screen.getByText("300")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(api.cancel).toHaveBeenCalled());

    act(() => applyImportEvent({ type: "import_cancelled", imported: 300, skipped: 38, failed: 2 }));
    expect(screen.getByRole("status")).toHaveTextContent("Import cancelled. 300 meetings imported");
    expect(screen.getByRole("button", { name: "Preview" })).toBeInTheDocument();
  });

  it("shows the counts, failed titles, and a Library link when done", async () => {
    api.status.mockResolvedValue(idle("••••wxyz"));
    renderSection();
    await screen.findByText("••••wxyz");
    act(() =>
      applyImportEvent({
        type: "import_complete",
        imported: 1362,
        skipped: 1,
        failed: 2,
        failed_titles: ["Weekly sync"],
      }),
    );
    expect(screen.getByText("1,362")).toBeInTheDocument();
    expect(screen.getByText("Weekly sync")).toBeInTheDocument();
    expect(screen.getByText("and 1 more")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open the Library" })).toHaveAttribute("href", "/");
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.getByRole("button", { name: "Preview" })).toBeInTheDocument();
  });

  it("shows the error when the import fails", async () => {
    api.status.mockResolvedValue(idle("••••wxyz"));
    renderSection();
    await screen.findByText("••••wxyz");
    act(() => applyImportEvent({ type: "import_error", error: "Granola is unreachable." }));
    expect(screen.getByRole("alert")).toHaveTextContent("Import failed. Granola is unreachable.");
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByRole("button", { name: "Preview" })).toBeInTheDocument();
  });

  it("resumes a run already in progress on load", async () => {
    api.status.mockResolvedValue({
      key: "••••wxyz",
      running: true,
      progress: { done: 10, total: 20, imported: 10, skipped: 0, failed: 0 },
      last: null,
    });
    renderSection();
    expect(await screen.findByText("10 of 20")).toBeInTheDocument();
  });
});
