import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { EchoDevicePicker } from "./EchoDevicePicker";
import { audioApi, settingsApi } from "../lib/api/settings";

vi.mock("../lib/api/settings", async () => {
  const actual =
    await vi.importActual<typeof import("../lib/api/settings")>(
      "../lib/api/settings",
    );
  return {
    ...actual,
    audioApi: {
      ...actual.audioApi,
      outputDevices: vi.fn(),
      setEcho: vi.fn(),
    },
    settingsApi: {
      ...actual.settingsApi,
      get: vi.fn(),
      patch: vi.fn(),
    },
  };
});

const DEVICES = [
  { name: "BlackHole 2ch", is_default: false, sample_rate: 48000 },
  { name: "MacBook Pro Speakers", is_default: true, sample_rate: 48000 },
];

function baseGeneral(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    port: 7878,
    open_browser_on_start: true,
    audio_input_device: "",
    audio_echo_output_device: "",
    audio_echo_enabled: false,
    audio_echo_buffer: 512,
    first_run_completed: true,
    stt_provider: "cloud",
    stt_model: "nova-3",
    meeting_detection: true,
    ...overrides,
  };
}

function renderPicker(recording = true) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <EchoDevicePicker meetingId="meeting-1" recording={recording} />
    </QueryClientProvider>,
  );
}

describe("EchoDevicePicker", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(audioApi.outputDevices).mockResolvedValue(DEVICES);
    vi.mocked(audioApi.setEcho).mockResolvedValue({
      enabled: true,
      device: "BlackHole 2ch",
    });
    vi.mocked(settingsApi.get).mockResolvedValue({
      general: baseGeneral(),
      providers: [],
      presets: [],
      deepgram_key_masked: null,
    });
    vi.mocked(settingsApi.patch).mockResolvedValue(
      baseGeneral({ audio_echo_output_device: "BlackHole 2ch" }),
    );
  });

  it("renders a combobox with 'System default' and both device names", async () => {
    renderPicker();
    const select = await screen.findByRole("combobox", { name: /echo to/i });
    expect(screen.getByRole("option", { name: /system default/i })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /BlackHole 2ch/ })).toBeInTheDocument();
    expect(select).toHaveValue("");
  });

  it("calls setEcho(meetingId, { device }) while recording", async () => {
    renderPicker();
    const select = await screen.findByRole("combobox", { name: /echo to/i });
    fireEvent.change(select, { target: { value: "BlackHole 2ch" } });
    await waitFor(() => {
      expect(audioApi.setEcho).toHaveBeenCalledWith("meeting-1", {
        device: "BlackHole 2ch",
      });
    });
  });

  it("shows a switching indicator while pending and reflects the resolved device on success", async () => {
    let resolveSet!: (v: { enabled: boolean; device: string }) => void;
    vi.mocked(audioApi.setEcho).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveSet = resolve;
        }),
    );
    renderPicker();
    const select = await screen.findByRole("combobox", { name: /echo to/i });
    fireEvent.change(select, { target: { value: "BlackHole 2ch" } });

    await waitFor(() => expect(select).toBeDisabled());
    expect(screen.getByText(/switching/i)).toBeInTheDocument();

    resolveSet({ enabled: true, device: "BlackHole 2ch" });
    await waitFor(() => expect(select).toHaveValue("BlackHole 2ch"));
  });

  it("shows an inline error on mutation failure", async () => {
    vi.mocked(audioApi.setEcho).mockRejectedValue(new Error("device not found"));
    renderPicker();
    const select = await screen.findByRole("combobox", { name: /echo to/i });
    fireEvent.change(select, { target: { value: "BlackHole 2ch" } });

    await waitFor(() => {
      expect(screen.getByText(/device not found/i)).toBeInTheDocument();
    });
  });

  it("PATCHes settings.audio_echo_output_device (not the live endpoint) while stopped", async () => {
    renderPicker(false);
    const select = await screen.findByRole("combobox", { name: /echo to/i });
    fireEvent.change(select, { target: { value: "BlackHole 2ch" } });

    await waitFor(() => {
      expect(settingsApi.patch).toHaveBeenCalledWith({
        audio_echo_output_device: "BlackHole 2ch",
      });
    });
    expect(audioApi.setEcho).not.toHaveBeenCalled();
  });

  it("defaults the selection to the already-persisted audio_echo_output_device while stopped", async () => {
    vi.mocked(settingsApi.get).mockResolvedValue({
      general: baseGeneral({ audio_echo_output_device: "BlackHole 2ch" }),
      providers: [],
      presets: [],
      deepgram_key_masked: null,
    });

    renderPicker(false);
    const select = await screen.findByRole("combobox", { name: /echo to/i });
    await waitFor(() => expect(select).toHaveValue("BlackHole 2ch"));
  });
});
