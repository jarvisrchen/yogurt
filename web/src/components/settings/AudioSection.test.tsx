import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { AudioSection } from "./AudioSection";
import { audioApi, settingsApi, type General } from "../../lib/api/settings";

vi.mock("../../lib/api/settings", async () => {
  const actual =
    await vi.importActual<typeof import("../../lib/api/settings")>(
      "../../lib/api/settings",
    );
  return {
    ...actual,
    audioApi: {
      ...actual.audioApi,
      devices: vi.fn(),
      outputDevices: vi.fn(),
    },
    settingsApi: {
      ...actual.settingsApi,
      patch: vi.fn(),
    },
  };
});

function baseGeneral(overrides: Partial<General> = {}): General {
  return {
    port: 7878,
    open_browser_on_start: true,
    audio_input_device: "",
    audio_echo_feature: false,
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

function renderSection(general: General) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <AudioSection general={general} />
    </QueryClientProvider>,
  );
}

describe("AudioSection — AUD-13 mic echo feature toggle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(audioApi.devices).mockResolvedValue([]);
    vi.mocked(audioApi.outputDevices).mockResolvedValue([]);
    vi.mocked(settingsApi.patch).mockResolvedValue(baseGeneral());
  });

  it("hides the echo output device and echo buffer controls when the feature is off", () => {
    renderSection(baseGeneral({ audio_echo_feature: false }));

    expect(
      screen.getByRole("checkbox", { name: /mic echo/i }),
    ).not.toBeChecked();
    expect(screen.queryByText("Echo output device")).toBeNull();
    expect(screen.queryByText("Echo buffer")).toBeNull();
  });

  it("shows the echo output device and echo buffer controls when the feature is on", () => {
    renderSection(baseGeneral({ audio_echo_feature: true }));

    expect(
      screen.getByRole("checkbox", { name: /mic echo/i }),
    ).toBeChecked();
    expect(screen.getByText("Echo output device")).toBeInTheDocument();
    expect(screen.getByText("Echo buffer")).toBeInTheDocument();
  });

  it("PATCHes audio_echo_feature when the toggle is flipped", async () => {
    renderSection(baseGeneral({ audio_echo_feature: false }));

    fireEvent.click(screen.getByRole("checkbox", { name: /mic echo/i }));

    await waitFor(() => {
      expect(settingsApi.patch).toHaveBeenCalledWith({
        audio_echo_feature: true,
      });
    });
  });
});
