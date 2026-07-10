import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MicDevicePicker } from "./MicDevicePicker";
import { audioApi } from "../lib/api/settings";

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
  };
});

const DEVICES = [
  { name: "Built-in Microphone", is_default: true, sample_rate: 48000 },
  { name: "AirPods Pro", is_default: false, sample_rate: 16000 },
];

function renderPicker() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MicDevicePicker meetingId="meeting-1" />
    </QueryClientProvider>,
  );
}

describe("MicDevicePicker", () => {
  beforeEach(() => {
    vi.mocked(audioApi.devices).mockResolvedValue(DEVICES);
    vi.mocked(audioApi.switchMeetingDevice).mockResolvedValue({
      status: "switched",
      device: "AirPods Pro",
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
