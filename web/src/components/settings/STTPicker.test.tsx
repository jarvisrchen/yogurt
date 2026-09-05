/**
 * STTPicker test suite (fast task: Deepgram key field + fake-pill removal).
 *
 * Covers:
 *  - AssemblyAI/Groq pills are gone (only Deepgram is real).
 *  - `deepgram_key_masked` renders the masked-key UX (mirrors ProviderCard).
 *  - Pasting a key + clicking "Save key" posts via `settingsApi.setSttKey`.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { SettingsView } from "../../lib/api/settings";
import type { ModelView } from "../../lib/api/stt";

function fixture(overrides: Partial<SettingsView> = {}): SettingsView {
  return {
    general: {
      port: 7878,
      open_browser_on_start: true,
      audio_input_device: "",
        audio_echo_output_device: "",
        audio_echo_enabled: false,
        audio_echo_buffer: 512,
      first_run_completed: true,
      stt_provider: "cloud",
      stt_model: "small.en",
      meeting_detection: true,
    },
    providers: [],
    presets: [],
    deepgram_key_masked: null,
    ...overrides,
  };
}

const settingsApiMock = vi.hoisted(() => ({
  get: vi.fn(),
  patch: vi.fn(),
  setSttKey: vi.fn(),
  testSttKey: vi.fn(),
}));

vi.mock("../../lib/api/settings", () => ({
  settingsApi: settingsApiMock,
}));

const modelsFixture: ModelView[] = [
  { name: "small.en", size_mb: 487, downloaded: true, intel_supported: true, managed_by_homebrew: false },
];

vi.mock("../../lib/api/stt", () => ({
  useModels: vi.fn(() => ({
    data: modelsFixture,
    isLoading: false,
    isError: false,
    error: null,
  })),
  // LocalSTTCard renders <ModelDownloadDialog>, which calls
  // useDownloadModel() unconditionally — stub it so the module mock is
  // complete even though no test here triggers a download.
  useDownloadModel: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
  // LocalSTTCard also calls useDeleteModel() unconditionally for the
  // ModelPicker trash affordance - stub it even though no test here
  // deletes a model.
  useDeleteModel: vi.fn(() => ({
    mutate: vi.fn(),
    isPending: false,
    isError: false,
    isSuccess: false,
    error: null,
    data: undefined,
    variables: undefined,
  })),
}));

import { STTPicker } from "./STTPicker";

function renderPicker() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <STTPicker />
    </QueryClientProvider>,
  );
}

describe("STTPicker — Cloud card", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    settingsApiMock.get.mockResolvedValue(fixture());
    settingsApiMock.setSttKey.mockResolvedValue(undefined);
    settingsApiMock.testSttKey.mockResolvedValue({ ok: true });
  });

  it("does not render the fake AssemblyAI/Groq pills", async () => {
    renderPicker();
    await waitFor(() => screen.getByTestId("cloud-stt-card"));
    expect(screen.queryByText(/assemblyai/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/groq/i)).not.toBeInTheDocument();
  });

  it("shows 'No key stored yet' when deepgram_key_masked is null", async () => {
    renderPicker();
    await waitFor(() => screen.getByTestId("cloud-stt-card"));
    expect(screen.getByText(/no key stored yet/i)).toBeInTheDocument();
  });

  it("renders the masked key + stored badge when deepgram_key_masked is set", async () => {
    settingsApiMock.get.mockResolvedValue(
      fixture({ deepgram_key_masked: "••••ABCD" }),
    );
    renderPicker();
    await waitFor(() => screen.getByText("••••ABCD"));
    expect(screen.getByText(/✓ stored/)).toBeInTheDocument();
  });

  it("posts the pasted key via settingsApi.setSttKey on Save key", async () => {
    renderPicker();
    await waitFor(() => screen.getByTestId("cloud-stt-card"));

    const input = screen.getByPlaceholderText(/paste key…/i);
    fireEvent.change(input, { target: { value: "dg-secret-123" } });
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));

    await waitFor(() => {
      expect(settingsApiMock.setSttKey).toHaveBeenCalledWith("dg-secret-123");
    });
  });

  it("shows the 'applies to next recording' notice", async () => {
    renderPicker();
    await waitFor(() => screen.getByTestId("cloud-stt-card"));
    expect(
      screen.getByText(/changes apply to the next recording/i),
    ).toBeInTheDocument();
  });

  it("surfaces the server's 422 message when a PATCH is rejected", async () => {
    settingsApiMock.patch.mockRejectedValue(
      new Error(
        '422 Unprocessable Entity: {"error":"local model medium.en is not downloaded - download it in Settings > Transcription first"}',
      ),
    );
    renderPicker();
    await waitFor(() => screen.getByTestId("cloud-stt-card"));

    // "small.en" is downloaded per `modelsFixture`, so "Use Local" is the
    // clickable transition from the fixture's default `stt_provider:
    // "cloud"` — clicking an already-checked radio wouldn't fire onChange.
    fireEvent.click(screen.getByRole("radio", { name: /use local/i }));

    await waitFor(() => {
      expect(screen.getByTestId("stt-patch-error")).toHaveTextContent(
        "local model medium.en is not downloaded - download it in Settings > Transcription first",
      );
    });
    // The raw status-code-prefixed JSON blob must not leak into the UI.
    expect(screen.queryByText(/422 unprocessable/i)).not.toBeInTheDocument();
  });

  it("tests the stored key via settingsApi.testSttKey when the draft is empty", async () => {
    settingsApiMock.get.mockResolvedValue(
      fixture({ deepgram_key_masked: "••••ABCD" }),
    );
    renderPicker();
    await waitFor(() => screen.getByText("••••ABCD"));

    fireEvent.click(
      screen.getByRole("button", { name: /test connection for deepgram/i }),
    );

    await waitFor(() => {
      expect(settingsApiMock.testSttKey).toHaveBeenCalledWith(undefined);
    });
    expect(await screen.findByText(/✓ connection works/i)).toBeInTheDocument();
  });

  it("hides the verdict once the draft no longer matches what was tested", async () => {
    settingsApiMock.get.mockResolvedValue(
      fixture({ deepgram_key_masked: "••••ABCD" }),
    );
    renderPicker();
    await waitFor(() => screen.getByText("••••ABCD"));

    fireEvent.click(
      screen.getByRole("button", { name: /test connection for deepgram/i }),
    );
    expect(await screen.findByText(/✓ connection works/i)).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText(/paste key…/i), {
      target: { value: "dg-new-draft" },
    });

    expect(screen.queryByText(/✓ connection works/i)).not.toBeInTheDocument();
  });
});
