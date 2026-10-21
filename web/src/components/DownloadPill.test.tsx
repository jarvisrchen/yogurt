import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent, act } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MemoryRouter } from "react-router";

const navigateSpy = vi.hoisted(() => vi.fn());
vi.mock("react-router", async () => {
  const actual = await vi.importActual<typeof import("react-router")>("react-router");
  return { ...actual, useNavigate: () => navigateSpy };
});

import { DownloadPill } from "./DownloadPill";
import {
  READY_MS,
  applyDownloadEvent,
  dismissDownload,
  useModelDownload,
  type DownloadEvent,
} from "../hooks/useModelDownload";

function DialogProbe() {
  return <span data-testid="dialog-open">{String(useModelDownload()?.dialogOpen ?? false)}</span>;
}

function renderPill() {
  return render(
    <MemoryRouter>
      <DownloadPill />
      <DialogProbe />
    </MemoryRouter>,
  );
}

const onModelsChanged = vi.fn();
const send = (ev: DownloadEvent) => act(() => applyDownloadEvent(ev, onModelsChanged));
const progress = (model: string, done: number, total = 200): DownloadEvent => ({
  type: "stt_model_download_progress",
  model,
  bytes_downloaded: done,
  total_bytes: total,
  bytes_per_sec: 10,
  eta_seconds: 5,
});

afterEach(() => {
  cleanup();
  act(() => dismissDownload());
  navigateSpy.mockClear();
  onModelsChanged.mockClear();
  vi.useRealTimers();
});

describe("DownloadPill", () => {
  it("adopts a download it did not start and shows its percentage", () => {
    renderPill();
    expect(screen.queryByTestId("download-pill")).not.toBeInTheDocument();

    send(progress("large-v3", 50));
    expect(screen.getByTestId("download-pill")).toHaveTextContent("large-v3");
    expect(screen.getByTestId("download-pill")).toHaveTextContent("25%");
  });

  it("ignores another model's frames while one is shown", () => {
    renderPill();
    send(progress("large-v3", 50));
    send(progress("small.en", 190));
    expect(screen.getByTestId("download-pill")).toHaveTextContent("large-v3");
    expect(screen.getByTestId("download-pill")).toHaveTextContent("25%");
  });

  it("opens the dialog on click while downloading", () => {
    renderPill();
    send(progress("large-v3", 50));
    fireEvent.click(screen.getByRole("button", { name: /show download progress for large-v3/i }));
    expect(screen.getByTestId("dialog-open")).toHaveTextContent("true");
  });

  it("shows ready on complete, refreshes the models list, then hides", () => {
    vi.useFakeTimers();
    renderPill();
    send(progress("large-v3", 199));
    send({ type: "stt_model_download_complete", model: "large-v3" });

    expect(screen.getByTestId("download-pill")).toHaveTextContent("ready");
    expect(onModelsChanged).toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(READY_MS));
    expect(screen.queryByTestId("download-pill")).not.toBeInTheDocument();
  });

  it("keeps an error up until clicked", () => {
    vi.useFakeTimers();
    renderPill();
    send(progress("large-v3", 50));
    send({ type: "stt_model_download_error", model: "large-v3", error: "boom" });
    act(() => vi.advanceTimersByTime(READY_MS * 10));

    const pill = screen.getByRole("button", { name: /show download error for large-v3/i });
    expect(pill).toHaveTextContent("failed");
    fireEvent.click(pill);
    expect(screen.getByTestId("dialog-open")).toHaveTextContent("true");
  });

  it("disappears when the download is cancelled", () => {
    renderPill();
    send(progress("large-v3", 50));
    send({ type: "stt_model_download_cancelled", model: "large-v3" });
    expect(screen.queryByTestId("download-pill")).not.toBeInTheDocument();
    expect(onModelsChanged).toHaveBeenCalled();
  });
});
