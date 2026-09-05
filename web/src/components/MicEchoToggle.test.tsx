import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MicEchoToggle } from "./MicEchoToggle";
import { audioApi } from "../lib/api/settings";
import { activeRecordingKey } from "../lib/api/meetings";

vi.mock("../lib/api/settings", async () => {
  const actual =
    await vi.importActual<typeof import("../lib/api/settings")>(
      "../lib/api/settings",
    );
  return {
    ...actual,
    audioApi: {
      ...actual.audioApi,
      setEcho: vi.fn(),
    },
  };
});

function renderToggle(echoEnabled: boolean, recording = true) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  qc.setQueryData(activeRecordingKey, {
    id: "meeting-1",
    title: "Weekly sync",
    started_at: 0,
    mic_muted: false,
    echo_enabled: echoEnabled,
  });
  return {
    qc,
    ...render(
      <QueryClientProvider client={qc}>
        <MicEchoToggle meetingId="meeting-1" recording={recording} />
      </QueryClientProvider>,
    ),
  };
}

describe("MicEchoToggle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders as 'Echo mic' when off", async () => {
    renderToggle(false);
    expect(
      await screen.findByRole("button", { name: /echo mic/i }),
    ).toBeInTheDocument();
  });

  it("renders as 'Stop echo' when on", async () => {
    renderToggle(true);
    expect(
      await screen.findByRole("button", { name: /stop echo/i }),
    ).toBeInTheDocument();
  });

  it("is disabled (not unmounted) while not recording", async () => {
    renderToggle(false, false);
    const button = await screen.findByRole("button", { name: /echo mic/i });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute(
      "title",
      expect.stringMatching(/available once recording starts/i),
    );
  });

  it("calls setEcho(meetingId, { enabled: true }) when clicked while off", async () => {
    vi.mocked(audioApi.setEcho).mockResolvedValue({
      enabled: true,
      device: "",
    });
    renderToggle(false);
    const button = await screen.findByRole("button", { name: /echo mic/i });
    fireEvent.click(button);
    await waitFor(() => {
      expect(audioApi.setEcho).toHaveBeenCalledWith("meeting-1", { enabled: true });
    });
  });

  it("calls setEcho(meetingId, { enabled: false }) when clicked while on", async () => {
    vi.mocked(audioApi.setEcho).mockResolvedValue({
      enabled: false,
      device: "",
    });
    renderToggle(true);
    const button = await screen.findByRole("button", { name: /stop echo/i });
    fireEvent.click(button);
    await waitFor(() => {
      expect(audioApi.setEcho).toHaveBeenCalledWith("meeting-1", { enabled: false });
    });
  });

  it("shows an inline error on mutation failure", async () => {
    vi.mocked(audioApi.setEcho).mockRejectedValue(
      new Error("device does not support 48kHz"),
    );
    renderToggle(false);
    const button = await screen.findByRole("button", { name: /echo mic/i });
    fireEvent.click(button);

    await waitFor(() => {
      expect(
        screen.getByText(/device does not support 48kHz/i),
      ).toBeInTheDocument();
    });
  });

  it("toggles on the 'E' hotkey while recording", async () => {
    vi.mocked(audioApi.setEcho).mockResolvedValue({
      enabled: true,
      device: "",
    });
    renderToggle(false, true);
    await screen.findByRole("button", { name: /echo mic/i });

    fireEvent.keyDown(window, { key: "e" });

    await waitFor(() => {
      expect(audioApi.setEcho).toHaveBeenCalledWith("meeting-1", { enabled: true });
    });
  });

  it("does not fire the 'E' hotkey while not recording", async () => {
    renderToggle(false, false);
    await screen.findByRole("button", { name: /echo mic/i });

    fireEvent.keyDown(window, { key: "e" });

    expect(audioApi.setEcho).not.toHaveBeenCalled();
  });

  it("does not fire the 'E' hotkey while a text input has focus", async () => {
    render(
      <div>
        <input aria-label="scratch" />
      </div>,
    );
    renderToggle(false, true);
    await screen.findByRole("button", { name: /echo mic/i });

    screen.getByLabelText("scratch").focus();
    fireEvent.keyDown(screen.getByLabelText("scratch"), { key: "e" });

    expect(audioApi.setEcho).not.toHaveBeenCalled();
  });
});
