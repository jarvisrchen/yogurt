import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MicMuteToggle } from "./MicMuteToggle";
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
      setMicMuted: vi.fn(),
    },
  };
});

function renderToggle(micMuted: boolean, recording = true) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  qc.setQueryData(activeRecordingKey, {
    id: "meeting-1",
    title: "Weekly sync",
    started_at: 0,
    mic_muted: micMuted,
  });
  return {
    qc,
    ...render(
      <QueryClientProvider client={qc}>
        <MicMuteToggle meetingId="meeting-1" recording={recording} />
      </QueryClientProvider>,
    ),
  };
}

describe("MicMuteToggle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders as 'Pause mic' when unmuted", async () => {
    renderToggle(false);
    expect(
      await screen.findByRole("button", { name: /pause mic/i }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/mic paused/i)).not.toBeInTheDocument();
  });

  it("renders as 'Resume mic' with a 'Mic paused' note when muted", async () => {
    renderToggle(true);
    expect(
      await screen.findByRole("button", { name: /resume mic/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/mic paused/i)).toBeInTheDocument();
  });

  it("is disabled (not unmounted) while not recording", async () => {
    renderToggle(false, false);
    const button = await screen.findByRole("button", { name: /pause mic/i });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute(
      "title",
      expect.stringMatching(/available once recording starts/i),
    );
  });

  it("calls setMicMuted(meetingId, true) when clicked while unmuted", async () => {
    vi.mocked(audioApi.setMicMuted).mockResolvedValue({
      status: "ok",
      muted: true,
    });
    renderToggle(false);
    const button = await screen.findByRole("button", { name: /pause mic/i });
    fireEvent.click(button);
    await waitFor(() => {
      expect(audioApi.setMicMuted).toHaveBeenCalledWith("meeting-1", true);
    });
  });

  it("calls setMicMuted(meetingId, false) when clicked while muted", async () => {
    vi.mocked(audioApi.setMicMuted).mockResolvedValue({
      status: "ok",
      muted: false,
    });
    renderToggle(true);
    const button = await screen.findByRole("button", { name: /resume mic/i });
    fireEvent.click(button);
    await waitFor(() => {
      expect(audioApi.setMicMuted).toHaveBeenCalledWith("meeting-1", false);
    });
  });

  it("disables the button while the mutation is pending", async () => {
    let resolveSet!: (v: { status: string; muted: boolean }) => void;
    vi.mocked(audioApi.setMicMuted).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveSet = resolve;
        }),
    );
    renderToggle(false);
    const button = await screen.findByRole("button", { name: /pause mic/i });
    fireEvent.click(button);

    await waitFor(() => expect(button).toBeDisabled());
    resolveSet({ status: "ok", muted: true });
    await waitFor(() => expect(button).not.toBeDisabled());
  });

  it("shows an inline error on mutation failure", async () => {
    vi.mocked(audioApi.setMicMuted).mockRejectedValue(
      new Error("meeting is not currently recording"),
    );
    renderToggle(false);
    const button = await screen.findByRole("button", { name: /pause mic/i });
    fireEvent.click(button);

    await waitFor(() => {
      expect(
        screen.getByText(/meeting is not currently recording/i),
      ).toBeInTheDocument();
    });
  });

  it("toggles on the 'M' hotkey while recording", async () => {
    vi.mocked(audioApi.setMicMuted).mockResolvedValue({
      status: "ok",
      muted: true,
    });
    renderToggle(false, true);
    await screen.findByRole("button", { name: /pause mic/i });

    fireEvent.keyDown(window, { key: "m" });

    await waitFor(() => {
      expect(audioApi.setMicMuted).toHaveBeenCalledWith("meeting-1", true);
    });
  });

  it("does not fire the 'M' hotkey while not recording", async () => {
    renderToggle(false, false);
    await screen.findByRole("button", { name: /pause mic/i });

    fireEvent.keyDown(window, { key: "m" });

    expect(audioApi.setMicMuted).not.toHaveBeenCalled();
  });

  it("does not fire the 'M' hotkey while a text input has focus", async () => {
    render(
      <div>
        <input aria-label="scratch" />
      </div>,
    );
    renderToggle(false, true);
    await screen.findByRole("button", { name: /pause mic/i });

    screen.getByLabelText("scratch").focus();
    fireEvent.keyDown(screen.getByLabelText("scratch"), { key: "m" });

    expect(audioApi.setMicMuted).not.toHaveBeenCalled();
  });
});
