import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MicDevicePicker } from "./MicDevicePicker";
import { audioApi, settingsApi } from "../lib/api/settings";

vi.mock("../lib/api/settings", async () => {
  const actual =
    await vi.importActual<typeof import("../lib/api/settings")>(
      "../lib/api/settings",
    );
  return {
    ...actual,
    audioApi: {
      devices: vi.fn(),
      switchMeetingDevice: vi.fn(),
    },
    settingsApi: {
      ...actual.settingsApi,
      get: vi.fn(),
      patch: vi.fn(),
    },
  };
});

const DEVICES = [
  { name: "Built-in Microphone", is_default: true, sample_rate: 48000 },
  { name: "AirPods Pro", is_default: false, sample_rate: 16000 },
];

function renderPicker(recording = true) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MicDevicePicker meetingId="meeting-1" recording={recording} />
    </QueryClientProvider>,
  );
}

describe("MicDevicePicker", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(audioApi.devices).mockResolvedValue(DEVICES);
    vi.mocked(audioApi.switchMeetingDevice).mockResolvedValue({
      status: "switched",
      device: "AirPods Pro",
    });
    vi.mocked(settingsApi.get).mockResolvedValue({
      general: {
        port: 7878,
        open_browser_on_start: true,
        audio_input_device: "",
        first_run_completed: true,
        stt_provider: "cloud",
        stt_model: "nova-3",
        meeting_detection: true,
      },
      providers: [],
      presets: [],
      deepgram_key_masked: null,
    });
    vi.mocked(settingsApi.patch).mockResolvedValue({
      port: 7878,
      open_browser_on_start: true,
      audio_input_device: "AirPods Pro",
      first_run_completed: true,
      stt_provider: "cloud",
      stt_model: "nova-3",
      meeting_detection: true,
    });
  });

  it("renders a combobox with both device names, defaulting to the default device", async () => {
    renderPicker();
    const select = await screen.findByRole("combobox", {
      name: /microphone/i,
    });
    expect(
      screen.getByRole("option", { name: /Built-in Microphone/ }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("option", { name: /AirPods Pro/ }),
    ).toBeInTheDocument();
    expect(select).toHaveValue("Built-in Microphone");
  });

  it("calls switchMeetingDevice with (meetingId, deviceName) on selection", async () => {
    renderPicker();
    const select = await screen.findByRole("combobox", {
      name: /microphone/i,
    });
    fireEvent.change(select, { target: { value: "AirPods Pro" } });
    await waitFor(() => {
      expect(audioApi.switchMeetingDevice).toHaveBeenCalledWith(
        "meeting-1",
        "AirPods Pro",
      );
    });
  });

  it("reflects the resolved active device after a successful switch", async () => {
    renderPicker();
    const select = await screen.findByRole("combobox", {
      name: /microphone/i,
    });
    fireEvent.change(select, { target: { value: "AirPods Pro" } });
    await waitFor(() => {
      expect(select).toHaveValue("AirPods Pro");
    });
  });

  it("disables the select and shows a switching indicator while pending, and surfaces an error without reverting the value", async () => {
    let resolveSwitch!: (v: { status: string; device: string }) => void;
    vi.mocked(audioApi.switchMeetingDevice).mockReset();
    vi.mocked(audioApi.switchMeetingDevice).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveSwitch = resolve;
        }),
    );

    renderPicker();
    const select = await screen.findByRole("combobox", {
      name: /microphone/i,
    });
    fireEvent.change(select, { target: { value: "AirPods Pro" } });

    await waitFor(() => {
      expect(select).toBeDisabled();
    });
    expect(screen.getByText(/switching/i)).toBeInTheDocument();

    resolveSwitch({ status: "switched", device: "AirPods Pro" });
    await waitFor(() => {
      expect(select).not.toBeDisabled();
    });
  });

  it("shows an inline error and keeps the last-known-good value on mutation error", async () => {
    vi.mocked(audioApi.switchMeetingDevice).mockReset();
    vi.mocked(audioApi.switchMeetingDevice).mockRejectedValue(
      new Error("device not found"),
    );

    renderPicker();
    const select = await screen.findByRole("combobox", {
      name: /microphone/i,
    });
    fireEvent.change(select, { target: { value: "AirPods Pro" } });

    await waitFor(() => {
      expect(screen.getByText(/device not found/i)).toBeInTheDocument();
    });
    expect(select).toHaveValue("Built-in Microphone");
  });
});

describe("MicDevicePicker — stopped (meeting open, not recording)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(audioApi.devices).mockResolvedValue(DEVICES);
    vi.mocked(settingsApi.get).mockResolvedValue({
      general: {
        port: 7878,
        open_browser_on_start: true,
        audio_input_device: "",
        first_run_completed: true,
        stt_provider: "cloud",
        stt_model: "nova-3",
        meeting_detection: true,
      },
      providers: [],
      presets: [],
      deepgram_key_masked: null,
    });
    vi.mocked(settingsApi.patch).mockResolvedValue({
      port: 7878,
      open_browser_on_start: true,
      audio_input_device: "AirPods Pro",
      first_run_completed: true,
      stt_provider: "cloud",
      stt_model: "nova-3",
      meeting_detection: true,
    });
  });

  it("renders the combobox while stopped instead of hiding", async () => {
    renderPicker(false);
    expect(
      await screen.findByRole("combobox", { name: /microphone/i }),
    ).toBeInTheDocument();
  });

  it("PATCHes settings.audio_input_device (not the live hot-swap endpoint) on selection while stopped", async () => {
    renderPicker(false);
    const select = await screen.findByRole("combobox", {
      name: /microphone/i,
    });
    fireEvent.change(select, { target: { value: "AirPods Pro" } });

    await waitFor(() => {
      expect(settingsApi.patch).toHaveBeenCalledWith({
        audio_input_device: "AirPods Pro",
      });
    });
    expect(audioApi.switchMeetingDevice).not.toHaveBeenCalled();
  });

  it("defaults the selection to the already-persisted audio_input_device, not just the OS default", async () => {
    vi.mocked(settingsApi.get).mockResolvedValue({
      general: {
        port: 7878,
        open_browser_on_start: true,
        audio_input_device: "AirPods Pro",
        first_run_completed: true,
        stt_provider: "cloud",
        stt_model: "nova-3",
        meeting_detection: true,
      },
      providers: [],
      presets: [],
      deepgram_key_masked: null,
    });

    renderPicker(false);
    const select = await screen.findByRole("combobox", {
      name: /microphone/i,
    });
    await waitFor(() => expect(select).toHaveValue("AirPods Pro"));
  });

  it("reflects the newly-selected device after a successful settings patch, so the next Start honors it", async () => {
    renderPicker(false);
    const select = await screen.findByRole("combobox", {
      name: /microphone/i,
    });
    fireEvent.change(select, { target: { value: "AirPods Pro" } });
    await waitFor(() => {
      expect(select).toHaveValue("AirPods Pro");
    });
  });
});
